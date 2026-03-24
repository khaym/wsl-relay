use std::io::Cursor;

use wsl_relay::clipboard::{ClipboardBackend, StubClipboard, dib_to_png};

// --- StubClipboard fixture sanity checks ---

#[test]
fn stub_returns_decodable_png() {
    let stub = StubClipboard;
    let data = stub.read_image().unwrap();
    assert!(!data.is_empty(), "PNG bytes should not be empty");

    let png_signature = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    assert_eq!(&data[..8], png_signature, "Should have full PNG signature");

    let decoder = png::Decoder::new(Cursor::new(&data));
    let reader = decoder.read_info().expect("Should decode PNG header");
    let info = reader.info();
    assert_eq!(info.width, 1);
    assert_eq!(info.height, 1);
}

// --- dib_to_png tests ---

/// Helper: build a minimal BITMAPINFOHEADER (40 bytes) + pixel data
fn make_dib(width: i32, height: i32, bit_count: u16, pixels: &[u8]) -> Vec<u8> {
    let header_size: u32 = 40;
    let mut dib = Vec::new();
    dib.extend_from_slice(&header_size.to_le_bytes()); // biSize
    dib.extend_from_slice(&width.to_le_bytes()); // biWidth
    dib.extend_from_slice(&height.to_le_bytes()); // biHeight
    dib.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    dib.extend_from_slice(&bit_count.to_le_bytes()); // biBitCount
    dib.extend_from_slice(&0u32.to_le_bytes()); // biCompression
    dib.extend_from_slice(&0u32.to_le_bytes()); // biSizeImage
    dib.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    dib.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    dib.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    dib.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant
    dib.extend_from_slice(pixels);
    dib
}

/// Decode PNG bytes and return (width, height, color_type, raw pixels)
fn decode_png(data: &[u8]) -> (u32, u32, png::ColorType, Vec<u8>) {
    let decoder = png::Decoder::new(Cursor::new(data));
    let mut reader = decoder.read_info().expect("decode PNG header");
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).expect("decode PNG frame");
    buf.truncate(info.buffer_size());
    (info.width, info.height, info.color_type, buf)
}

// -- Boundary: header validation --

#[test]
fn dib_to_png_rejects_too_small_header() {
    let result = dib_to_png(&[0u8; 39]);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("BITMAPINFOHEADER"));
}

#[test]
fn dib_to_png_rejects_zero_width() {
    let dib = make_dib(0, 1, 24, &[]);
    let result = dib_to_png(&dib);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Invalid DIB width")
    );
}

#[test]
fn dib_to_png_rejects_negative_width() {
    let dib = make_dib(-1, 1, 24, &[]);
    let result = dib_to_png(&dib);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Invalid DIB width")
    );
}

#[test]
fn dib_to_png_rejects_zero_height() {
    let dib = make_dib(1, 0, 24, &[]);
    let result = dib_to_png(&dib);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Invalid DIB height")
    );
}

#[test]
fn dib_to_png_rejects_unsupported_bpp() {
    let dib = make_dib(1, 1, 8, &[0u8; 4]);
    let result = dib_to_png(&dib);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unsupported DIB bit depth")
    );
}

#[test]
fn dib_to_png_rejects_header_size_exceeding_buffer() {
    // Craft DIB where header_size claims 1000 but buffer is only 44 bytes
    let mut dib = make_dib(1, 1, 24, &[0u8; 4]);
    dib[0..4].copy_from_slice(&1000u32.to_le_bytes()); // biSize = 1000
    let result = dib_to_png(&dib);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("header_size"));
}

#[test]
fn dib_to_png_rejects_truncated_pixel_data() {
    // 2x2 24bpp needs 2 rows * 8 bytes stride = 16 bytes of pixel data, give only 4
    let dib = make_dib(2, 2, 24, &[0u8; 4]);
    let result = dib_to_png(&dib);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("pixel data too small")
    );
}

// -- Color conversion --

#[test]
fn dib_to_png_converts_24bpp_bgr_to_rgb() {
    // 1x1: DIB stores BGR = (B=0xFF, G=0x00, R=0x80), expect RGB = (0x80, 0x00, 0xFF)
    // 24bpp row: 3 bytes + 1 byte padding = 4 bytes stride
    let pixels = [0xFF, 0x00, 0x80, 0x00]; // [B, G, R, pad]
    let dib = make_dib(1, 1, 24, &pixels);
    let png_data = dib_to_png(&dib).unwrap();

    let (w, h, ct, px) = decode_png(&png_data);
    assert_eq!((w, h), (1, 1));
    assert_eq!(ct, png::ColorType::Rgb);
    let expected_rgb = [0x80, 0x00, 0xFF];
    assert_eq!(&px[..3], &expected_rgb);
}

#[test]
fn dib_to_png_converts_2wide_24bpp() {
    // 2x1 24bpp: stride = ceil(2*24/32)*4 = 8 bytes
    // px0: BGR (0x10, 0x20, 0x30), px1: BGR (0x40, 0x50, 0x60), pad 2 bytes
    let pixels = [0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x00, 0x00];
    let dib = make_dib(2, 1, 24, &pixels);
    let png_data = dib_to_png(&dib).unwrap();

    let (w, h, ct, px) = decode_png(&png_data);
    assert_eq!((w, h), (2, 1));
    assert_eq!(ct, png::ColorType::Rgb);
    // px0: BGR(0x10,0x20,0x30) → RGB(0x30,0x20,0x10)
    assert_eq!(&px[0..3], &[0x30, 0x20, 0x10]);
    // px1: BGR(0x40,0x50,0x60) → RGB(0x60,0x50,0x40)
    assert_eq!(&px[3..6], &[0x60, 0x50, 0x40]);
}

#[test]
fn dib_to_png_converts_32bpp_bgra_to_rgba() {
    // 1x1: BGRA = (B=0xFF, G=0x00, R=0x80, A=0xCC)
    let pixels = [0xFF, 0x00, 0x80, 0xCC];
    let dib = make_dib(1, 1, 32, &pixels);
    let png_data = dib_to_png(&dib).unwrap();

    let (w, h, ct, px) = decode_png(&png_data);
    assert_eq!((w, h), (1, 1));
    assert_eq!(ct, png::ColorType::Rgba);
    let expected_rgba = [0x80, 0x00, 0xFF, 0xCC];
    assert_eq!(&px[..4], &expected_rgba);
}

// -- Row ordering --

#[test]
fn dib_to_png_bottom_up_row_order() {
    // 1-wide, 2-tall, bottom-up (height=2 > 0)
    // DIB row 0 (bottom of image): BGR red   (B=0x00, G=0x00, R=0xFF)
    // DIB row 1 (top of image):    BGR blue  (B=0xFF, G=0x00, R=0x00)
    let red_bgr = [0x00, 0x00, 0xFF, 0x00]; // + pad
    let blue_bgr = [0xFF, 0x00, 0x00, 0x00]; // + pad
    let pixels: Vec<u8> = [red_bgr, blue_bgr].concat();
    let dib = make_dib(1, 2, 24, &pixels);
    let png_data = dib_to_png(&dib).unwrap();

    let (w, h, _, px) = decode_png(&png_data);
    assert_eq!((w, h), (1, 2));
    let red_rgb = [0xFF, 0x00, 0x00];
    let blue_rgb = [0x00, 0x00, 0xFF];
    // PNG row 0 (top) = blue (was DIB row 1)
    assert_eq!(&px[0..3], &blue_rgb);
    // PNG row 1 (bottom) = red (was DIB row 0)
    assert_eq!(&px[3..6], &red_rgb);
}

#[test]
fn dib_to_png_top_down_row_order() {
    // 1-wide, 2-tall, top-down (height=-2 < 0)
    // DIB row 0 (top of image):    BGR green (B=0x00, G=0xFF, R=0x00)
    // DIB row 1 (bottom of image): BGR red   (B=0x00, G=0x00, R=0xFF)
    let green_bgr = [0x00, 0xFF, 0x00, 0x00]; // + pad
    let red_bgr = [0x00, 0x00, 0xFF, 0x00]; // + pad
    let pixels: Vec<u8> = [green_bgr, red_bgr].concat();
    let dib = make_dib(1, -2, 24, &pixels);
    let png_data = dib_to_png(&dib).unwrap();

    let (w, h, _, px) = decode_png(&png_data);
    assert_eq!((w, h), (1, 2));
    let green_rgb = [0x00, 0xFF, 0x00];
    let red_rgb = [0xFF, 0x00, 0x00];
    // PNG preserves top-down order
    assert_eq!(&px[0..3], &green_rgb);
    assert_eq!(&px[3..6], &red_rgb);
}

// -- Row padding --

#[test]
fn dib_to_png_handles_row_padding_single_row() {
    // 3-wide, 24bpp: 3*3=9 bytes → padded to 12 bytes per row
    // px0: BGR(0xFF,0x00,0x00), px1: BGR(0x00,0xFF,0x00), px2: BGR(0x00,0x00,0xFF)
    let pixels = [
        0xFF, 0x00, 0x00, // px0
        0x00, 0xFF, 0x00, // px1
        0x00, 0x00, 0xFF, // px2
        0x00, 0x00, 0x00, // 3 bytes padding to reach 12
    ];
    let dib = make_dib(3, 1, 24, &pixels);
    let png_data = dib_to_png(&dib).unwrap();

    let (w, h, _, px) = decode_png(&png_data);
    assert_eq!((w, h), (3, 1));
    // px0: BGR(0xFF,0x00,0x00) → RGB(0x00,0x00,0xFF)
    assert_eq!(&px[0..3], &[0x00, 0x00, 0xFF]);
    // px1: BGR(0x00,0xFF,0x00) → RGB(0x00,0xFF,0x00)
    assert_eq!(&px[3..6], &[0x00, 0xFF, 0x00]);
    // px2: BGR(0x00,0x00,0xFF) → RGB(0xFF,0x00,0x00)
    assert_eq!(&px[6..9], &[0xFF, 0x00, 0x00]);
}

#[test]
fn dib_to_png_handles_row_padding_multi_row() {
    // 3-wide, 2-tall, 24bpp: stride=12, bottom-up
    // Row 0 (bottom): all white BGR(0xFF,0xFF,0xFF)
    // Row 1 (top): all black BGR(0x00,0x00,0x00)
    let white_row = [
        0xFF, 0xFF, 0xFF, // px0
        0xFF, 0xFF, 0xFF, // px1
        0xFF, 0xFF, 0xFF, // px2
        0x00, 0x00, 0x00, // pad
    ];
    let black_row = [
        0x00, 0x00, 0x00, // px0
        0x00, 0x00, 0x00, // px1
        0x00, 0x00, 0x00, // px2
        0x00, 0x00, 0x00, // pad
    ];
    let pixels: Vec<u8> = [white_row.as_slice(), black_row.as_slice()].concat();
    let dib = make_dib(3, 2, 24, &pixels);
    let png_data = dib_to_png(&dib).unwrap();

    let (w, h, _, px) = decode_png(&png_data);
    assert_eq!((w, h), (3, 2));
    // PNG row 0 (top) = black (was DIB row 1)
    assert!(px[0..9].iter().all(|&b| b == 0x00));
    // PNG row 1 (bottom) = white (was DIB row 0)
    assert!(px[9..18].iter().all(|&b| b == 0xFF));
}
