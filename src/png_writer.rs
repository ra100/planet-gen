use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PngRowFormat {
    Gray8,
    Gray16,
    Rgba8,
}

impl PngRowFormat {
    fn color_type(self) -> png::ColorType {
        match self {
            Self::Gray8 | Self::Gray16 => png::ColorType::Grayscale,
            Self::Rgba8 => png::ColorType::Rgba,
        }
    }

    fn bit_depth(self) -> png::BitDepth {
        match self {
            Self::Gray16 => png::BitDepth::Sixteen,
            Self::Gray8 | Self::Rgba8 => png::BitDepth::Eight,
        }
    }

    fn row_bytes(self, width: u32) -> Result<usize, String> {
        let bytes_per_pixel = match self {
            Self::Gray8 => 1,
            Self::Gray16 => 2,
            Self::Rgba8 => 4,
        };
        usize::try_from(width)
            .map_err(|_| "PNG width exceeds usize")?
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| "PNG scanline size overflows usize".into())
    }
}

pub struct AtomicScanlinePngWriter {
    writer: Option<png::StreamWriter<'static, BufWriter<File>>>,
    staging: PathBuf,
    output: PathBuf,
    row_bytes: usize,
    height: u32,
    next_row: u32,
}

impl AtomicScanlinePngWriter {
    pub fn create(
        path: &Path,
        width: u32,
        height: u32,
        format: PngRowFormat,
    ) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("PNG dimensions must be positive".into());
        }
        let staging = path.with_extension("png.part");
        let _ = std::fs::remove_file(&staging);
        let file = BufWriter::new(
            File::create(&staging)
                .map_err(|error| format!("failed to create PNG staging file: {error}"))?,
        );
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(format.color_type());
        encoder.set_depth(format.bit_depth());
        let writer = encoder
            .write_header()
            .map_err(|error| format!("failed to write PNG header: {error}"))?
            .into_stream_writer()
            .map_err(|error| format!("failed to start PNG stream: {error}"))?;
        Ok(Self {
            writer: Some(writer),
            staging,
            output: path.to_owned(),
            row_bytes: format.row_bytes(width)?,
            height,
            next_row: 0,
        })
    }

    pub fn write_scanline(&mut self, row: &[u8]) -> Result<(), String> {
        if row.len() != self.row_bytes {
            return Err("PNG scanline has an unexpected byte count".into());
        }
        if self.next_row == self.height {
            return Err("all declared PNG scanlines were already written".into());
        }
        self.writer
            .as_mut()
            .ok_or("PNG writer is already finalized")?
            .write_all(row)
            .map_err(|error| format!("failed to write PNG scanline: {error}"))?;
        self.next_row += 1;
        Ok(())
    }

    pub fn finish(mut self) -> Result<(), String> {
        if self.next_row != self.height {
            return Err("cannot publish PNG before all declared scanlines are written".into());
        }
        self.writer
            .take()
            .ok_or("PNG writer is already finalized")?
            .finish()
            .map_err(|error| format!("failed to finalize PNG: {error}"))?;
        std::fs::rename(&self.staging, &self.output)
            .map_err(|error| format!("failed to publish PNG: {error}"))
    }
}

impl Drop for AtomicScanlinePngWriter {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.staging);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;

    #[test]
    fn streams_16_bit_rows_with_the_declared_png_depth() {
        let path = std::env::temp_dir().join(format!(
            "planet-gen-png-writer-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut writer =
            AtomicScanlinePngWriter::create(&path, 2, 2, PngRowFormat::Gray16).unwrap();
        writer.write_scanline(&[0, 0, 0xff, 0xff]).unwrap();
        writer.write_scanline(&[0x80, 0, 0x40, 0]).unwrap();
        writer.finish().unwrap();

        let decoder = png::Decoder::new(BufReader::new(File::open(&path).unwrap()));
        let reader = decoder.read_info().unwrap();
        assert_eq!(reader.info().bit_depth, png::BitDepth::Sixteen);
        assert_eq!(reader.info().color_type, png::ColorType::Grayscale);
        assert!(!path.with_extension("png.part").exists());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn incomplete_or_dropped_pngs_are_not_published() {
        let path = std::env::temp_dir().join(format!(
            "planet-gen-png-drop-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let staging = path.with_extension("png.part");
        let mut writer = AtomicScanlinePngWriter::create(&path, 1, 2, PngRowFormat::Gray8).unwrap();
        writer.write_scanline(&[0]).unwrap();
        assert!(writer.finish().is_err());
        assert!(!path.exists());
        assert!(!staging.exists());
    }
}
