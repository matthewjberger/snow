use nightshade::prelude::wgpu;

/// Copies a whole texture level into a mapped buffer and returns its bytes.
pub fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    bytes_per_texel: u32,
) -> Vec<u8> {
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

    let (sender, receiver) = std::sync::mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).expect("readback channel");
        });
    loop {
        if device.poll(wgpu::PollType::Poll).is_err() {
            break;
        }
        match receiver.try_recv() {
            Ok(result) => {
                result.expect("readback map");
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
        }
    }

    let mapped = staging.slice(..).get_mapped_range();
    let mut out = Vec::with_capacity((unpadded * height) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        out.extend_from_slice(&mapped[start..start + unpadded as usize]);
    }
    drop(mapped);
    staging.unmap();
    out
}
