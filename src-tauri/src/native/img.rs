//! PNG encoding and thumbnail scaling.

pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.set_compression(png::Compression::Fast);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
        writer.finish().ok()?;
    }
    Some(out)
}

pub fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width, info.height);
    let px = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks_exact(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks_exact(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&c| [c, c, c, 255]).collect(),
        _ => return None,
    };
    Some((w, h, rgba))
}

/// Box-filter downscale so that the longest side is at most `max`.
pub fn shrink(width: u32, height: u32, rgba: &[u8], max: u32) -> (u32, u32, Vec<u8>) {
    let longest = width.max(height);
    if longest <= max {
        return (width, height, rgba.to_vec());
    }
    let scale = longest as f32 / max as f32;
    let nw = ((width as f32 / scale).round() as u32).max(1);
    let nh = ((height as f32 / scale).round() as u32).max(1);
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        let y0 = (y as f32 * scale) as u32;
        let y1 = (((y + 1) as f32 * scale) as u32).min(height).max(y0 + 1);
        for x in 0..nw {
            let x0 = (x as f32 * scale) as u32;
            let x1 = (((x + 1) as f32 * scale) as u32).min(width).max(x0 + 1);
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let o = ((sy * width + sx) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += rgba[o + c] as u32;
                    }
                    n += 1;
                }
            }
            let o = ((y * nw + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    (nw, nh, out)
}
