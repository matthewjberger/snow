use nightshade::prelude::wgpu;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A texture copy on its way back to the CPU.
///
/// Started and polled across frames, because the mapping completes only once the
/// runtime gets a turn: on the web that means returning to the browser's event
/// loop. Both platforms take the same path, and the boot sequence spends a frame
/// per readback.
pub struct Readback {
    staging: wgpu::Buffer,
    ready: Arc<AtomicBool>,
    width: u32,
    height: u32,
    bytes_per_texel: u32,
}

/// Copies a whole texture level toward the CPU and asks for the mapping.
pub fn begin_read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    bytes_per_texel: u32,
) -> Readback {
    const ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let unpadded = width * bytes_per_texel;
    let padded = unpadded.div_ceil(ALIGNMENT) * ALIGNMENT;

    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snow_readback"),
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("snow_readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let ready = Arc::new(AtomicBool::new(false));
    let signal = ready.clone();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            if result.is_ok() {
                signal.store(true, Ordering::Release);
            }
        });

    Readback {
        staging,
        ready,
        width,
        height,
        bytes_per_texel,
    }
}

/// The bytes, once the mapping has landed. `None` means try again next frame.
pub fn poll_read(readback: &Readback, device: &wgpu::Device) -> Option<Vec<u8>> {
    // Native drives the callback from this poll. On the web the browser calls
    // back on its own.
    if device.poll(wgpu::PollType::Poll).is_err() {
        return None;
    }
    if !readback.ready.load(Ordering::Acquire) {
        return None;
    }

    const ALIGNMENT: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let unpadded = readback.width * readback.bytes_per_texel;
    let padded = unpadded.div_ceil(ALIGNMENT) * ALIGNMENT;

    let mapped = readback.staging.slice(..).get_mapped_range();
    let mut out = Vec::with_capacity((unpadded * readback.height) as usize);
    for row in 0..readback.height {
        let start = (row * padded) as usize;
        out.extend_from_slice(&mapped[start..start + unpadded as usize]);
    }
    drop(mapped);
    readback.staging.unmap();
    Some(out)
}
