use anyhow::bail;

pub trait ClipboardBackend: Send + Sync {
    fn read_image(&self) -> anyhow::Result<Vec<u8>>;
}

pub struct StubClipboard;

impl ClipboardBackend for StubClipboard {
    fn read_image(&self) -> anyhow::Result<Vec<u8>> {
        Ok(minimal_png())
    }
}

/// Return a minimal valid 1x1 white PNG (67 bytes).
fn minimal_png() -> Vec<u8> {
    let mut buf = Vec::new();

    // PNG signature
    buf.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);

    // IHDR chunk: 1x1, 8-bit RGB
    let ihdr_data: [u8; 13] = [
        0x00, 0x00, 0x00, 0x01, // width = 1
        0x00, 0x00, 0x00, 0x01, // height = 1
        0x08, // bit depth = 8
        0x02, // color type = RGB
        0x00, // compression
        0x00, // filter
        0x00, // interlace
    ];
    write_chunk(&mut buf, b"IHDR", &ihdr_data);

    // IDAT chunk: zlib-compressed scanline (filter=0, R=255, G=255, B=255)
    let idat_compressed: [u8; 14] = [
        0x78, 0x9C, 0x62, 0xF8, 0x0F, 0x00, 0x00, 0x04, 0x00, 0x01, // deflate data
        0x02, 0x02, 0x00, 0x05, // adler32
    ];
    write_chunk(&mut buf, b"IDAT", &idat_compressed);

    // IEND chunk
    write_chunk(&mut buf, b"IEND", &[]);

    buf
}

fn write_chunk(buf: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    let len = (data.len() as u32).to_be_bytes();
    buf.extend_from_slice(&len);
    buf.extend_from_slice(chunk_type);
    buf.extend_from_slice(data);

    // CRC32 over chunk_type + data
    let crc = crc32(chunk_type, data);
    buf.extend_from_slice(&crc.to_be_bytes());
}

fn crc32(chunk_type: &[u8; 4], data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFFFFFF;
    for &byte in chunk_type.iter().chain(data.iter()) {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    crc ^ 0xFFFFFFFF
}

#[cfg(target_os = "windows")]
pub struct WindowsClipboard;

#[cfg(target_os = "windows")]
impl ClipboardBackend for WindowsClipboard {
    fn read_image(&self) -> anyhow::Result<Vec<u8>> {
        use std::slice;

        use anyhow::{Context, bail};
        use windows::Win32::Foundation::HGLOBAL;
        use windows::Win32::System::DataExchange::{
            CloseClipboard, GetClipboardData, OpenClipboard,
        };
        use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

        unsafe {
            OpenClipboard(None).context("Failed to open clipboard")?;

            // CF_DIB = 8
            let result = (|| -> anyhow::Result<Vec<u8>> {
                let handle = GetClipboardData(8);
                if handle.is_err() {
                    bail!("No image data in clipboard (CF_DIB not available)");
                }
                let handle = handle.unwrap();
                let hmem = HGLOBAL(handle.0);

                let ptr = GlobalLock(hmem);
                if ptr.is_null() {
                    bail!("Failed to lock clipboard memory");
                }

                let size = GlobalSize(hmem);
                if size == 0 {
                    GlobalUnlock(hmem);
                    bail!("Clipboard memory block has zero size");
                }

                // Copy data out before unlocking so dib_to_png errors
                // don't leak the GlobalLock
                let data = slice::from_raw_parts(ptr as *const u8, size);
                let dib_copy = data.to_vec();
                GlobalUnlock(hmem);

                dib_to_png(&dib_copy)
            })();

            let _ = CloseClipboard();
            result
        }
    }
}

/// Convert a DIB (BITMAPINFOHEADER + pixel data) to PNG bytes.
/// Supports 24bpp (BGR) and 32bpp (BGRA).
pub fn dib_to_png(dib: &[u8]) -> anyhow::Result<Vec<u8>> {
    use std::io::Cursor;

    if dib.len() < 40 {
        bail!("DIB data too small for BITMAPINFOHEADER");
    }

    let header_size = u32::from_le_bytes(dib[0..4].try_into().unwrap()) as usize;
    let width = i32::from_le_bytes(dib[4..8].try_into().unwrap());
    let height = i32::from_le_bytes(dib[8..12].try_into().unwrap());
    let bit_count = u16::from_le_bytes(dib[14..16].try_into().unwrap());

    if width <= 0 {
        bail!("Invalid DIB width: {width}");
    }
    if height == 0 {
        bail!("Invalid DIB height: 0");
    }

    let (height_abs, top_down) = if height < 0 {
        ((-height) as u32, true)
    } else {
        (height as u32, false)
    };
    let width = width as u32;

    let (color_type, channels) = match bit_count {
        24 => (png::ColorType::Rgb, 3u32),
        32 => (png::ColorType::Rgba, 4u32),
        _ => bail!("Unsupported DIB bit depth: {bit_count}bpp (only 24/32 supported)"),
    };

    // Validate header_size does not exceed buffer
    if header_size > dib.len() {
        bail!(
            "DIB header_size ({header_size}) exceeds buffer length ({})",
            dib.len()
        );
    }
    let pixel_data = &dib[header_size..];

    // DIB rows are padded to 4-byte boundaries
    let row_stride = (width * (bit_count as u32)).div_ceil(32) as usize * 4;

    // Validate pixel data is large enough for all rows
    let required_size = height_abs as usize * row_stride;
    if pixel_data.len() < required_size {
        bail!(
            "DIB pixel data too small: need {required_size} bytes, got {}",
            pixel_data.len()
        );
    }

    let mut png_buf = Cursor::new(Vec::new());
    {
        let mut encoder = png::Encoder::new(&mut png_buf, width, height_abs);
        encoder.set_color(color_type);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;

        let mut img_data = Vec::with_capacity((width * height_abs * channels) as usize);

        for y in 0..height_abs {
            let src_y = if top_down {
                y as usize
            } else {
                (height_abs - 1 - y) as usize
            };
            let row_start = src_y * row_stride;

            for x in 0..width as usize {
                let px = row_start + x * (bit_count as usize / 8);
                // DIB is BGR(A), PNG needs RGB(A)
                let b = pixel_data[px];
                let g = pixel_data[px + 1];
                let r = pixel_data[px + 2];
                img_data.push(r);
                img_data.push(g);
                img_data.push(b);
                if channels == 4 {
                    let a = pixel_data[px + 3];
                    img_data.push(a);
                }
            }
        }

        writer.write_image_data(&img_data)?;
    }

    Ok(png_buf.into_inner())
}
