use std::ffi::{CString, c_char};
use std::path::Path;

unsafe extern "C" {
    fn planet_gen_exr_open(path: *const c_char, width: i32, height: i32) -> *mut std::ffi::c_void;
    fn planet_gen_exr_write_rgba_scanline(
        writer: *mut std::ffi::c_void,
        y: i32,
        rgba: *const f32,
    ) -> i32;
    fn planet_gen_exr_close(writer: *mut std::ffi::c_void);
    fn planet_gen_exr_last_error() -> *const c_char;
}

fn native_error() -> String {
    let error = unsafe { planet_gen_exr_last_error() };
    if error.is_null() {
        "unknown native OpenEXR error".into()
    } else {
        unsafe { std::ffi::CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    }
}

pub struct ScanlineExrWriter {
    writer: *mut std::ffi::c_void,
    width: usize,
    height: usize,
    next_row: usize,
}

pub struct AtomicScanlineExrWriter {
    writer: Option<ScanlineExrWriter>,
    staging: std::path::PathBuf,
    output: std::path::PathBuf,
}

impl AtomicScanlineExrWriter {
    pub fn create(path: &Path, width: u32, height: u32) -> Result<Self, String> {
        let staging = path.with_extension("exr.part");
        let _ = std::fs::remove_file(&staging);
        Ok(Self {
            writer: Some(ScanlineExrWriter::create(&staging, width, height)?),
            staging,
            output: path.to_owned(),
        })
    }

    pub fn write_rgba_scanline(&mut self, pixels: &[f32]) -> Result<(), String> {
        self.writer
            .as_mut()
            .ok_or("EXR writer is already finalized")?
            .write_rgba_scanline(pixels)
    }

    pub fn finish(mut self) -> Result<(), String> {
        if self
            .writer
            .as_ref()
            .is_some_and(|writer| writer.next_row != writer.height)
        {
            return Err("cannot publish EXR before all declared scanlines are written".into());
        }
        self.writer.take();
        std::fs::rename(&self.staging, &self.output)
            .map_err(|error| format!("failed to publish EXR: {error}"))
    }
}

impl Drop for AtomicScanlineExrWriter {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.staging);
    }
}

impl ScanlineExrWriter {
    pub fn create(path: &Path, width: u32, height: u32) -> Result<Self, String> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err("EXR dimensions must fit positive native i32 values".into());
        }
        let path = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| "EXR output path contains a NUL byte")?;
        let writer = unsafe { planet_gen_exr_open(path.as_ptr(), width as i32, height as i32) };
        if writer.is_null() {
            return Err(format!(
                "OpenEXR failed to create scanline output: {}",
                native_error()
            ));
        }
        Ok(Self {
            writer,
            width: width as usize,
            height: height as usize,
            next_row: 0,
        })
    }

    pub fn write_rgba_scanline(&mut self, pixels: &[f32]) -> Result<(), String> {
        if pixels.len() != self.width * 4 {
            return Err("EXR scanline has an unexpected channel count".into());
        }
        if self.next_row == self.height {
            return Err("all declared EXR scanlines were already written".into());
        }
        let written = unsafe {
            planet_gen_exr_write_rgba_scanline(self.writer, self.next_row as i32, pixels.as_ptr())
        };
        if written == 0 {
            return Err(format!(
                "OpenEXR failed to write scanline: {}",
                native_error()
            ));
        }
        self.next_row += 1;
        Ok(())
    }
}

impl Drop for ScanlineExrWriter {
    fn drop(&mut self) {
        unsafe { planet_gen_exr_close(self.writer) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_lossless_rgba_and_publishes_atomically() {
        let path =
            std::env::temp_dir().join(format!("planet-gen-openexr-{}.exr", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut writer = AtomicScanlineExrWriter::create(&path, 2, 2).unwrap();
        writer
            .write_rgba_scanline(&[0.0, 0.25, 0.5, 1.0, 1.0, 0.75, 0.5, 1.0])
            .unwrap();
        writer
            .write_rgba_scanline(&[0.1, 0.2, 0.3, 1.0, 0.4, 0.5, 0.6, 1.0])
            .unwrap();
        writer.finish().unwrap();
        assert!(path.is_file());
        let image = exr::prelude::read_first_rgba_layer_from_file(
            &path,
            |size, _| vec![vec![[0.0; 4]; size.width()]; size.height()],
            |pixels, position, rgba: (f32, f32, f32, f32)| {
                pixels[position.y()][position.x()] = rgba.into()
            },
        )
        .unwrap();
        assert_eq!(
            image.layer_data.encoding.compression,
            exr::prelude::Compression::ZIP16
        );
        assert_eq!(
            image.layer_data.channel_data.pixels[1][1],
            [0.4, 0.5, 0.6, 1.0]
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn drop_removes_unpublished_output() {
        let path = std::env::temp_dir().join(format!(
            "planet-gen-openexr-drop-{}.exr",
            std::process::id()
        ));
        let staging = path.with_extension("exr.part");
        let _ = std::fs::remove_file(&path);
        {
            let _writer = AtomicScanlineExrWriter::create(&path, 1, 1).unwrap();
        }
        assert!(!path.exists());
        assert!(!staging.exists());
    }

    #[test]
    fn incomplete_output_is_cleaned_without_publication() {
        let path = std::env::temp_dir().join(format!(
            "planet-gen-openexr-incomplete-{}.exr",
            std::process::id()
        ));
        let staging = path.with_extension("exr.part");
        let mut writer = AtomicScanlineExrWriter::create(&path, 1, 2).unwrap();
        writer.write_rgba_scanline(&[0.0, 0.0, 0.0, 1.0]).unwrap();
        assert!(writer.finish().is_err());
        assert!(!path.exists());
        assert!(!staging.exists());
    }

    #[test]
    fn rejects_invalid_dimensions_before_ffi() {
        assert!(ScanlineExrWriter::create(Path::new("/tmp/invalid.exr"), 0, 1).is_err());
    }
}
