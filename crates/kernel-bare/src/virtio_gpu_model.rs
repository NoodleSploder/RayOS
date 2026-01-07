//! Minimal virtio-gpu device model (scanout-focused).
//!
//! This is a deliberately small subset intended for early bring-up:
//! - Tracks 2D resources and a single backing region.
//! - Publishes scanout metadata to the UI via `GuestScanoutPublisher`.
//! - Treats `TRANSFER_TO_HOST_2D` / `RESOURCE_FLUSH` as "frame ready" signals.
//!
//! NOTE: This file does not yet integrate with an in-kernel VMM or virtqueue
//! transport. The VMM layer will be responsible for:
//! - Providing request/response buffers (guest memory views)
//! - Delivering controlq commands here
//! - Mapping guest-physical addresses to a host-physical backing buffer

#![allow(dead_code)]

use crate::guest_surface::GuestSurface;
use crate::vmm::GuestScanoutPublisher;

use crate::virtio_gpu_proto as proto;

#[derive(Copy, Clone)]
struct Resource2d {
    id: u32,
    width: u32,
    height: u32,
    format: u32,
    backing_addr: u64,
    backing_len: u32,
}

impl Resource2d {
    const fn empty() -> Self {
        Self {
            id: 0,
            width: 0,
            height: 0,
            format: 0,
            backing_addr: 0,
            backing_len: 0,
        }
    }

    fn is_valid(&self) -> bool {
        self.id != 0 && self.width != 0 && self.height != 0
    }
}

pub struct VirtioGpuModel {
    publisher: GuestScanoutPublisher,
    resources: [Resource2d; 32],
    scanout_resource_id: u32,
    scanout_w: u32,
    scanout_h: u32,
}

impl VirtioGpuModel {
    pub const fn new(publisher: GuestScanoutPublisher) -> Self {
        Self {
            publisher,
            resources: [Resource2d::empty(); 32],
            scanout_resource_id: 0,
            scanout_w: 0,
            scanout_h: 0,
        }
    }

    fn find_slot_mut(&mut self, rid: u32) -> Option<&mut Resource2d> {
        for slot in &mut self.resources {
            if slot.id == rid {
                return Some(slot);
            }
        }
        None
    }

    fn find_slot(&self, rid: u32) -> Option<&Resource2d> {
        for slot in &self.resources {
            if slot.id == rid {
                return Some(slot);
            }
        }
        None
    }

    fn alloc_slot(&mut self, rid: u32) -> Option<&mut Resource2d> {
        if self.find_slot(rid).is_some() {
            return self.find_slot_mut(rid);
        }
        for slot in &mut self.resources {
            if slot.id == 0 {
                slot.id = rid;
                return Some(slot);
            }
        }
        None
    }

    fn free_slot(&mut self, rid: u32) {
        for slot in &mut self.resources {
            if slot.id == rid {
                *slot = Resource2d::empty();
                if self.scanout_resource_id == rid {
                    self.scanout_resource_id = 0;
                }
                return;
            }
        }
    }

    fn format_to_bpp(format: u32) -> Option<u32> {
        match format {
            proto::VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM
            | proto::VIRTIO_GPU_FORMAT_B8G8R8X8_UNORM
            | proto::VIRTIO_GPU_FORMAT_R8G8B8A8_UNORM
            | proto::VIRTIO_GPU_FORMAT_R8G8B8X8_UNORM => Some(32),
            _ => None,
        }
    }

    fn publish_scanout_from(&self, res: &Resource2d) {
        let Some(bpp) = Self::format_to_bpp(res.format) else {
            return;
        };

        // Milestone-1 assumption: scanout is a tightly packed 32bpp buffer.
        // Stride will be refined later when we support multiple mem entries or
        // explicit stride negotiation.
        let stride_px = res.width;

        let surface = GuestSurface {
            width: res.width,
            height: res.height,
            stride_px,
            bpp,
            backing_phys: res.backing_addr,
        };
        self.publisher.publish_scanout(surface);
    }

    // --- Device-model entry points (transport will call these) ---

    pub fn handle_get_display_info(&mut self, out: &mut proto::VirtioGpuRespDisplayInfo) {
        out.hdr.type_ = proto::VIRTIO_GPU_RESP_OK_DISPLAY_INFO;
        out.hdr.flags = 0;
        out.hdr.fence_id = 0;
        out.hdr.ctx_id = 0;
        out.hdr.padding = 0;

        // Default a single enabled display mode. Linux will typically pick a
        // preferred size later based on DRM/KMS; for bring-up, 1024x768 is a
        // reasonable baseline.
        let mut pmodes = [proto::VirtioGpuDisplayOne::disabled(); 16];
        pmodes[0] = proto::VirtioGpuDisplayOne {
            r: proto::VirtioGpuRect {
                x: 0,
                y: 0,
                width: 1024,
                height: 768,
            },
            enabled: 1,
            flags: 0,
        };
        out.pmodes = pmodes;
    }

    pub fn handle_resource_create_2d(
        &mut self,
        req: &proto::VirtioGpuResourceCreate2d,
        resp_hdr: &mut proto::VirtioGpuCtrlHdr,
    ) {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:RESOURCE_CREATE_2D rid=");
        crate::serial_write_hex_u64(req.resource_id as u64);
        crate::serial_write_str(" w=");
        crate::serial_write_hex_u64(req.width as u64);
        crate::serial_write_str(" h=");
        crate::serial_write_hex_u64(req.height as u64);
        crate::serial_write_str(" fmt=");
        crate::serial_write_hex_u64(req.format as u64);
        crate::serial_write_str("\n");

        let Some(slot) = self.alloc_slot(req.resource_id) else {
            resp_hdr.type_ = proto::VIRTIO_GPU_RESP_ERR_OUT_OF_MEMORY;
            return;
        };
        slot.width = req.width;
        slot.height = req.height;
        slot.format = req.format;
        slot.backing_addr = 0;
        slot.backing_len = 0;
        resp_hdr.type_ = proto::VIRTIO_GPU_RESP_OK_NODATA;
    }

    pub fn handle_resource_unref(
        &mut self,
        req: &proto::VirtioGpuResourceUnref,
        resp_hdr: &mut proto::VirtioGpuCtrlHdr,
    ) {
        self.free_slot(req.resource_id);
        resp_hdr.type_ = proto::VIRTIO_GPU_RESP_OK_NODATA;
    }

    pub fn handle_resource_attach_backing_single(
        &mut self,
        resource_id: u32,
        entry: proto::VirtioGpuMemEntry,
        resp_hdr: &mut proto::VirtioGpuCtrlHdr,
    ) {
        let is_scanout = self.scanout_resource_id == resource_id;

        crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:RESOURCE_ATTACH_BACKING rid=");
        crate::serial_write_hex_u64(resource_id as u64);
        crate::serial_write_str(" addr=");
        crate::serial_write_hex_u64(entry.addr);
        crate::serial_write_str(" len=");
        crate::serial_write_hex_u64(entry.length as u64);
        crate::serial_write_str(" scanout=");
        crate::serial_write_hex_u64(is_scanout as u64);
        crate::serial_write_str("\n");

        let Some(slot) = self.find_slot_mut(resource_id) else {
            resp_hdr.type_ = proto::VIRTIO_GPU_RESP_ERR_INVALID_RESOURCE_ID;
            return;
        };
        slot.backing_addr = entry.addr;
        slot.backing_len = entry.length;
        resp_hdr.type_ = proto::VIRTIO_GPU_RESP_OK_NODATA;

        let snapshot = *slot;
        if is_scanout && snapshot.is_valid() && snapshot.backing_addr != 0 {
            crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:PUBLISH_FROM_ATTACH rid=");
            crate::serial_write_hex_u64(resource_id as u64);
            crate::serial_write_str("\n");
            self.publish_scanout_from(&snapshot);
        }
    }

    pub fn handle_set_scanout(
        &mut self,
        req: &proto::VirtioGpuSetScanout,
        resp_hdr: &mut proto::VirtioGpuCtrlHdr,
    ) {
        crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:SET_SCANOUT id=");
        crate::serial_write_hex_u64(req.scanout_id as u64);
        crate::serial_write_str(" rid=");
        crate::serial_write_hex_u64(req.resource_id as u64);
        crate::serial_write_str(" rect=");
        crate::serial_write_hex_u64(req.rect.width as u64);
        crate::serial_write_str("x");
        crate::serial_write_hex_u64(req.rect.height as u64);
        crate::serial_write_str("\n");

        // We only support scanout 0 for now.
        if req.scanout_id != 0 {
            resp_hdr.type_ = proto::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER;
            return;
        }

        self.scanout_resource_id = req.resource_id;
        self.scanout_w = req.rect.width;
        self.scanout_h = req.rect.height;

        // Publish if the resource is already backed.
        if let Some(slot) = self.find_slot(req.resource_id) {
            let snapshot = *slot;
            if snapshot.backing_addr != 0 && snapshot.is_valid() {
                // Use the resource dimensions; rect may reflect a sub-rectangle.
                crate::serial_write_str("RAYOS_VMM:VIRTIO_GPU:PUBLISH_FROM_SET_SCANOUT rid=");
                crate::serial_write_hex_u64(req.resource_id as u64);
                crate::serial_write_str("\n");
                self.publish_scanout_from(&snapshot);
            }
        }

        resp_hdr.type_ = proto::VIRTIO_GPU_RESP_OK_NODATA;
    }

    pub fn handle_transfer_to_host_2d(
        &mut self,
        _req: &proto::VirtioGpuTransferToHost2d,
        resp_hdr: &mut proto::VirtioGpuCtrlHdr,
    ) {
        // Treat this as a "frame updated" signal for scanout.
        self.publisher.frame_ready();
        resp_hdr.type_ = proto::VIRTIO_GPU_RESP_OK_NODATA;
    }

    pub fn handle_resource_flush(
        &mut self,
        _req: &proto::VirtioGpuResourceFlush,
        resp_hdr: &mut proto::VirtioGpuCtrlHdr,
    ) {
        // Treat flush as a "present" point.
        self.publisher.frame_ready();
        resp_hdr.type_ = proto::VIRTIO_GPU_RESP_OK_NODATA;
    }

    pub fn scanout_dimensions(&self) -> (u32, u32) {
        (self.scanout_w, self.scanout_h)
    }
}
