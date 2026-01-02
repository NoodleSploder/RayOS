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
    }

    pub fn frame_ready(&self) {
        guest_surface::bump_frame_seq();
    }
}

//=============================================================================
// Feature-gated virtio-gpu device-model scaffolding.
//=============================================================================

#[cfg(feature = "vmm_virtio_gpu")]
pub mod virtio_gpu {
    use core::{mem, ptr};

    use crate::virtio_gpu_model::VirtioGpuModel;
    use crate::virtio_gpu_proto as proto;

    use super::GuestScanoutPublisher;

    #[inline(always)]
    unsafe fn phys_read_unaligned<T: Copy>(phys: u64) -> T {
        let p = phys as *const T;
        ptr::read_unaligned(p)
    }

    #[inline(always)]
    unsafe fn phys_write_unaligned<T: Copy>(phys: u64, v: T) {
        let p = phys as *mut T;
        ptr::write_unaligned(p, v)
    }

    #[inline(always)]
    fn init_resp_hdr_from_req(req: &proto::VirtioGpuCtrlHdr, resp_type: u32) -> proto::VirtioGpuCtrlHdr {
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

    impl VirtioGpuDevice {
        pub const fn new() -> Self {
            Self {
                model: VirtioGpuModel::new(GuestScanoutPublisher::new()),
            }
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
            if req_len < mem::size_of::<proto::VirtioGpuCtrlHdr>() || resp_len < mem::size_of::<proto::VirtioGpuCtrlHdr>() {
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
            match req_hdr.type_ {
                proto::VIRTIO_GPU_CMD_GET_DISPLAY_INFO => {
                    if resp_len < mem::size_of::<proto::VirtioGpuRespDisplayInfo>() {
                        let hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER);
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }

                    let mut out: proto::VirtioGpuRespDisplayInfo = mem::zeroed();
                    // Ensure fence propagation.
                    out.hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_DISPLAY_INFO);
                    self.model.handle_get_display_info(&mut out);
                    out.hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_DISPLAY_INFO);
                    phys_write_unaligned(resp_phys, out);
                    mem::size_of::<proto::VirtioGpuRespDisplayInfo>()
                }
                proto::VIRTIO_GPU_CMD_RESOURCE_CREATE_2D => {
                    if req_len < mem::size_of::<proto::VirtioGpuResourceCreate2d>() {
                        let hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER);
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let req: proto::VirtioGpuResourceCreate2d = phys_read_unaligned(req_phys);
                    let mut hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model.handle_resource_create_2d(&req, &mut hdr);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                proto::VIRTIO_GPU_CMD_RESOURCE_UNREF => {
                    if req_len < mem::size_of::<proto::VirtioGpuResourceUnref>() {
                        let hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER);
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let req: proto::VirtioGpuResourceUnref = phys_read_unaligned(req_phys);
                    let mut hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model.handle_resource_unref(&req, &mut hdr);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                proto::VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING => {
                    if req_len < mem::size_of::<proto::VirtioGpuResourceAttachBackingHdr>() + mem::size_of::<proto::VirtioGpuMemEntry>() {
                        let hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER);
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let base: proto::VirtioGpuResourceAttachBackingHdr = phys_read_unaligned(req_phys);
                    // Milestone-1: accept only the first entry.
                    let entry_phys = req_phys + mem::size_of::<proto::VirtioGpuResourceAttachBackingHdr>() as u64;
                    let entry: proto::VirtioGpuMemEntry = phys_read_unaligned(entry_phys);

                    let mut hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model
                        .handle_resource_attach_backing_single(base.resource_id, entry, &mut hdr);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                proto::VIRTIO_GPU_CMD_SET_SCANOUT => {
                    if req_len < mem::size_of::<proto::VirtioGpuSetScanout>() {
                        let hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER);
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let req: proto::VirtioGpuSetScanout = phys_read_unaligned(req_phys);
                    let mut hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model.handle_set_scanout(&req, &mut hdr);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                proto::VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D => {
                    if req_len < mem::size_of::<proto::VirtioGpuTransferToHost2d>() {
                        let hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER);
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let req: proto::VirtioGpuTransferToHost2d = phys_read_unaligned(req_phys);
                    let mut hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
                    self.model.handle_transfer_to_host_2d(&req, &mut hdr);
                    phys_write_unaligned(resp_phys, hdr);
                    mem::size_of::<proto::VirtioGpuCtrlHdr>()
                }
                proto::VIRTIO_GPU_CMD_RESOURCE_FLUSH => {
                    if req_len < mem::size_of::<proto::VirtioGpuResourceFlush>() {
                        let hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER);
                        phys_write_unaligned(resp_phys, hdr);
                        return mem::size_of::<proto::VirtioGpuCtrlHdr>();
                    }
                    let req: proto::VirtioGpuResourceFlush = phys_read_unaligned(req_phys);
                    let mut hdr = init_resp_hdr_from_req(&req_hdr, proto::VIRTIO_GPU_RESP_OK_NODATA);
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
