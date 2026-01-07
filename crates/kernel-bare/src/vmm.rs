use crate::guest_surface::{self, GuestSurface};

/// Minimal scaffolding for a future in-kernel VMM.
///
/// Today this is just an API boundary: a virtio-gpu (guest) implementation can
/// publish scanout metadata and notify frame readiness without knowing anything
/// about the compositor/UI.
pub struct GuestScanoutPublisher;

impl GuestScanoutPublisher {
    pub const fn new() -> Self {
        Self
    }

    pub fn publish_scanout(&self, surface: GuestSurface) {
        guest_surface::publish_surface(surface);
        crate::with_irqs_disabled(|| {
            // Emit a deterministic marker when the in-kernel VMM publishes a guest scanout.
            crate::serial_write_str("RAYOS_LINUX_DESKTOP_PRESENTED:ok:");
            crate::serial_write_hex_u64(surface.width as u64);
            crate::serial_write_str("x");
            crate::serial_write_hex_u64(surface.height as u64);
            crate::serial_write_str("\n");

            // Only claim the desktop is "presented" once we're both:
            //  - in Presented mode, and
            //  - have a scanout published.
            if guest_surface::presentation_state() == guest_surface::PresentationState::Presented {
                crate::serial_write_str("RAYOS_HOST_EVENT_V0:LINUX_PRESENTATION:PRESENTED\n");
            }
        });
    }

    pub fn frame_ready(&self) {
        // Bump the monotonic guest frame sequence and emit a first-frame marker
        // when the sequence transitions from 0 -> 1.
        let prev = guest_surface::frame_seq();
        guest_surface::bump_frame_seq();
        let now = guest_surface::frame_seq();
        crate::with_irqs_disabled(|| {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:FRAME_READY:");
            crate::serial_write_hex_u64(now);
            crate::serial_write_str("\n");
            if prev == 0 && now != 0 {
                crate::serial_write_str("RAYOS_LINUX_DESKTOP_FIRST_FRAME:ok:\n");
            }
        });
    }
}

//=============================================================================
// Feature-gated virtio-gpu device-model scaffolding.
//=============================================================================

#[cfg(feature = "vmm_virtio_gpu")]
pub mod virtio_gpu {
    use core::{mem, ptr};
    use core::sync::atomic::{AtomicUsize, Ordering};

    use crate::virtio_gpu_model::VirtioGpuModel;
    use crate::virtio_gpu_proto as proto;

    use super::GuestScanoutPublisher;

    #[inline(always)]
    unsafe fn phys_read_unaligned<T: Copy>(phys: u64) -> T {
        let p = crate::phys_to_virt(phys) as *const T;
        ptr::read_unaligned(p)
    }

    #[inline(always)]
    unsafe fn phys_write_unaligned<T: Copy>(phys: u64, v: T) {
        let p = crate::phys_to_virt(phys) as *mut T;
        ptr::write_unaligned(p, v)
    }

    #[inline(always)]
    fn init_resp_hdr_from_req(
        req: &proto::VirtioGpuCtrlHdr,
        resp_type: u32,
    ) -> proto::VirtioGpuCtrlHdr {
        // Preserve fence and ctx fields when present; flags are not echoed.
        proto::VirtioGpuCtrlHdr {
            type_: resp_type,
            flags: 0,
            fence_id: req.fence_id,
            ctx_id: req.ctx_id,
            padding: 0,
        }
    }

    pub struct VirtioGpuDevice {
        model: VirtioGpuModel,
    }

    static VIRTIO_GPU_REQ_LOG_COUNT: AtomicUsize = AtomicUsize::new(0);

    impl VirtioGpuDevice {
        pub const fn new() -> Self {
            Self {
                model: VirtioGpuModel::new(GuestScanoutPublisher::new()),
            }
        }

        /// Handle a single controlq request where request/response addresses are guest-physical
        /// (GPA) addresses.
        ///
        /// The caller provides a translation function that maps a GPA to a host physical
        /// address that is accessible via `phys_to_virt()`.
        ///
        /// This exists for the in-kernel VMM path where virtqueue descriptors contain GPAs.
        ///
        /// Safety:
        /// - `translate_gpa_to_phys` must return valid host physical addresses for the passed GPAs.
        /// - The translated ranges must be mapped and writable where required.
        pub unsafe fn handle_controlq_gpa(
            &mut self,
            req_gpa: u64,
            req_len: usize,
            resp_gpa: u64,
            resp_len: usize,
            translate_gpa_to_phys: fn(u64) -> Option<u64>,
        ) -> usize {
            let Some(req_phys) = translate_gpa_to_phys(req_gpa) else {
                return 0;
            };
            let Some(resp_phys) = translate_gpa_to_phys(resp_gpa) else {
                return 0;
            };

            if req_len < mem::size_of::<proto::VirtioGpuCtrlHdr>()
                || resp_len < mem::size_of::<proto::VirtioGpuCtrlHdr>()
            {
                let hdr = proto::VirtioGpuCtrlHdr {
                    type_: proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                    flags: 0,
                    fence_id: 0,
                    ctx_id: 0,
                    padding: 0,
                };
                phys_write_unaligned(resp_phys, hdr);
                return mem::size_of::<proto::VirtioGpuCtrlHdr>();
            }

            let req_hdr: proto::VirtioGpuCtrlHdr = phys_read_unaligned(req_phys);

            // Bounded request tracing for bring-up (GPA path). This is especially
            // important for RESOURCE_ATTACH_BACKING which is handled specially.
            let n = VIRTIO_GPU_REQ_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
            if n < 256 {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:GPA_REQ type=");
                crate::serial_write_hex_u64(req_hdr.type_ as u64);
                crate::serial_write_str(" len=");
                crate::serial_write_hex_u64(req_len as u64);
                crate::serial_write_str(" req_gpa=");
                crate::serial_write_hex_u64(req_gpa);
                crate::serial_write_str("\n");
            }

            if req_hdr.type_ != proto::VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING {
                return self.handle_controlq(req_phys, req_len, resp_phys, resp_len);
            }

            // Special-case RESOURCE_ATTACH_BACKING: translate embedded GPA backing address
            // into a host physical address so scanout publication uses CPU-visible memory.
            if req_len
                < mem::size_of::<proto::VirtioGpuResourceAttachBackingHdr>()
                    + mem::size_of::<proto::VirtioGpuMemEntry>()
            {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:ATTACH_BACKING:req_too_small\n");
                let hdr = init_resp_hdr_from_req(
                    &req_hdr,
                    proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                );
                phys_write_unaligned(resp_phys, hdr);
                return mem::size_of::<proto::VirtioGpuCtrlHdr>();
            }

            let base: proto::VirtioGpuResourceAttachBackingHdr = phys_read_unaligned(req_phys);
            let entry_phys = req_phys
                + mem::size_of::<proto::VirtioGpuResourceAttachBackingHdr>() as u64;
            let mut entry: proto::VirtioGpuMemEntry = phys_read_unaligned(entry_phys);

            crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:ATTACH_BACKING rid=");
            crate::serial_write_hex_u64(base.resource_id as u64);
            crate::serial_write_str(" n=");
            crate::serial_write_hex_u64(base.nr_entries as u64);
            crate::serial_write_str(" addr_gpa=");
            crate::serial_write_hex_u64(entry.addr);
            crate::serial_write_str(" len=");
            crate::serial_write_hex_u64(entry.length as u64);
            crate::serial_write_str("\n");

            let Some(backing_phys) = translate_gpa_to_phys(entry.addr) else {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:ATTACH_BACKING:translate_fail\n");
                let hdr = init_resp_hdr_from_req(
                    &req_hdr,
                    proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                );
                phys_write_unaligned(resp_phys, hdr);
                return mem::size_of::<proto::VirtioGpuCtrlHdr>();
            };
            entry.addr = backing_phys;

            let mut hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
            self.model
                .handle_resource_attach_backing_single(base.resource_id, entry, &mut hdr);
            phys_write_unaligned(resp_phys, hdr);
            mem::size_of::<proto::VirtioGpuCtrlHdr>()
        }

        /// Handle a single controlq request.
        ///
        /// This is a low-level building block for a future virtqueue transport:
        /// the VMM will pass the guest-physical address/length of the request and
        /// response buffers.
        ///
        /// Safety:
        /// - `req_phys..req_phys+req_len` and `resp_phys..resp_phys+resp_len` must
        ///   be valid, mapped ranges in the current address space.
        /// - Buffers must be writable for the response.
        pub unsafe fn handle_controlq(
            &mut self,
            req_phys: u64,
            req_len: usize,
            resp_phys: u64,
            resp_len: usize,
        ) -> usize {
            if req_len < mem::size_of::<proto::VirtioGpuCtrlHdr>()
                || resp_len < mem::size_of::<proto::VirtioGpuCtrlHdr>()
            {
                let hdr = proto::VirtioGpuCtrlHdr {
                    type_: proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                    flags: 0,
                    fence_id: 0,
                    ctx_id: 0,
                    padding: 0,
                };
                phys_write_unaligned(resp_phys, hdr);
                return mem::size_of::<proto::VirtioGpuCtrlHdr>();
            }

            let req_hdr: proto::VirtioGpuCtrlHdr = phys_read_unaligned(req_phys);

            // Bounded request tracing for bring-up.
            let n = VIRTIO_GPU_REQ_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
            if n < 256 {
                crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:REQ type=");
                crate::serial_write_hex_u64(req_hdr.type_ as u64);
                crate::serial_write_str(" len=");
                crate::serial_write_hex_u64(req_len as u64);
                crate::serial_write_str("\n");
            }
            match req_hdr.type_ {
                proto::VIRTIO_GPU_CMD_GET_DISPLAY_INFO => {
                    if resp_len < mem::size_of::<proto::VirtioGpuRespDisplayInfo>() {
                        let hdr = init_resp_hdr_from_req(
                            &req_hdr,
                            proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                        );
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }

                    let mut out: proto::VirtioGpuRespDisplayInfo = mem::zeroed();
                    // Ensure fence propagation.
                    out.hdr =
                        init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_DISPLAY_INFO);
                    self.model.handle_get_display_info(&mut out);
                    out.hdr =
                        init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_DISPLAY_INFO);
                    phys_write_unaligned(resp_phys, out);
                    mem::size_of::<proto::VirtioGpuRespDisplayInfo>()
                }
                proto::VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => {
                    if req_len < mem::size_of::<proto::VirtioGpuResourceCreate2d>() {
                        let hdr = init_resp_hdr_from_req(
                            &req_hdr,
                            proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                        );
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let req: proto::VirtioGpuResourceCreate2d = phys_read_unaligned(req_phys);
                    let mut hdr =
                        init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model.handle_resource_create_2d(&req, &mut hdr);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                proto::VIRTIO_GPU_CMD_RESOURCE_UNREF => {
                    if req_len < mem::size_of::<proto::VirtioGpuResourceUnref>() {
                        let hdr = init_resp_hdr_from_req(
                            &req_hdr,
                            proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                        );
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let req: proto::VirtioGpuResourceUnref = phys_read_unaligned(req_phys);
                    let mut hdr =
                        init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model.handle_resource_unref(&req, &mut hdr);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                proto::VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => {
                    if req_len
                        < mem::size_of::<proto::VirtioGpuResourceAttachBackingHdr>()
                            + mem::size_of::<proto::VirtioGpuMemEntry>()
                    {
                        let hdr = init_resp_hdr_from_req(
                            &req_hdr,
                            proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                        );
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let base: proto::VirtioGpuResourceAttachBackingHdr =
                        phys_read_unaligned(req_phys);
                    // Milestone-1: accept only the first entry.
                    let entry_phys = req_phys
                        + mem::size_of::<proto::VirtioGpuResourceAttachBackingHdr>() as u64;
                    let entry: proto::VirtioGpuMemEntry = phys_read_unaligned(entry_phys);

                    let mut hdr =
                        init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model.handle_resource_attach_backing_single(
                        base.resource_id,
                        entry,
                        &mut hdr,
                    );
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                proto::VIRTIO_GPU_CMD_SET_SCANOUT => {
                    if req_len < mem::size_of::<proto::VirtioGpuSetScanout>() {
                        let hdr = init_resp_hdr_from_req(
                            &req_hdr,
                            proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                        );
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let req: proto::VirtioGpuSetScanout = phys_read_unaligned(req_phys);
                    let mut hdr =
                        init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model.handle_set_scanout(&req, &mut hdr);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                proto::VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => {
                    if req_len < mem::size_of::<proto::VirtioGpuTransferToHost2d>() {
                        let hdr = init_resp_hdr_from_req(
                            &req_hdr,
                            proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                        );
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let req: proto::VirtioGpuTransferToHost2d = phys_read_unaligned(req_phys);
                    let mut hdr =
                        init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model.handle_transfer_to_host_2d(&req, &mut hdr);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                proto::VIRTIO_GPU_CMD_RESOURCE_FLUSH => {
                    if req_len < mem::size_of::<proto::VirtioGpuResourceFlush>() {
                        let hdr = init_resp_hdr_from_req(
                            &req_hdr,
                            proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER,
                        );
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let req: proto::VirtioGpuResourceFlush = phys_read_unaligned(req_phys);
                    let mut hdr =
                        init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model.handle_resource_flush(&req, &mut hdr);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                _ => {
                    let hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_ERR_UNSPEC);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
            }
        }

        pub fn scanout_dimensions(&self) -> (u32, u32) {
            self.model.scanout_dimensions()
        }
    }
}
