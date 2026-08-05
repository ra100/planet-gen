use std::ffi::{CString, c_char};
use std::path::{Path, PathBuf};

unsafe extern "C" {
    fn planet_gen_exr_open(
        path: *const c_char,
        width: i32,
        height: i32,
        channels: *const *const c_char,
        channel_count: i32,
    ) -> *mut std::ffi::c_void;
    fn planet_gen_exr_write_scanline(
        writer: *mut std::ffi::c_void,
        y: i32,
        pixels: *const f32,
        channels: *const *const c_char,
        channel_count: i32,
    ) -> i32;
    fn planet_gen_exr_close(writer: *mut std::ffi::c_void);
    fn planet_gen_exr_last_error() -> *const c_char;
}

fn native_error() -> String {
    // SAFETY: the native function returns a thread-local, NUL-terminated string valid until
    // the next native writer call on this thread.
    let error = unsafe { planet_gen_exr_last_error() };
    if error.is_null() {
        "unknown native OpenEXR error".into()
    } else {
        // SAFETY: native_error only dereferences the non-null C string returned above.
        unsafe { std::ffi::CStr::from_ptr(error) }
            .to_string_lossy()
            .into_owned()
    }
}

fn c_channels(channels: &[&str]) -> Result<Vec<CString>, String> {
    if channels.is_empty() || channels.len() > i32::MAX as usize {
        return Err("EXR requires between one and i32::MAX named channels".into());
    }
    let names: Vec<CString> = channels
        .iter()
        .map(|name| {
            if name.is_empty() {
                Err("EXR channel names must not be empty".into())
            } else {
                CString::new(*name).map_err(|_| "EXR channel name contains a NUL byte".into())
            }
        })
        .collect::<Result<_, String>>()?;
    if names.windows(2).any(|pair| pair[0] == pair[1])
        || names
            .iter()
            .enumerate()
            .any(|(index, name)| names[..index].contains(name))
    {
        return Err("EXR channel names must be unique".into());
    }
    Ok(names)
}

pub struct ScanlineExrWriter {
    writer: *mut std::ffi::c_void,
    width: usize,
    height: usize,
    next_row: usize,
    channels: Vec<CString>,
}

pub struct AtomicScanlineExrWriter {
    writer: Option<ScanlineExrWriter>,
    staging: PathBuf,
    output: PathBuf,
}

impl AtomicScanlineExrWriter {
    pub fn create(path: &Path, width: u32, height: u32, channels: &[&str]) -> Result<Self, String> {
        let staging = path.with_extension("exr.part");
        let _ = std::fs::remove_file(&staging);
        Ok(Self {
            writer: Some(ScanlineExrWriter::create(
                &staging, width, height, channels,
            )?),
            staging,
            output: path.to_owned(),
        })
    }

    pub fn write_scanline(&mut self, pixels: &[f32]) -> Result<(), String> {
        self.writer
            .as_mut()
            .ok_or("EXR writer is already finalized")?
            .write_scanline(pixels)
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
        // Close the native handle before deleting its path. Windows refuses the
        // deletion while the handle is live, and this also makes cleanup explicit.
        self.writer.take();
        let _ = std::fs::remove_file(&self.staging);
    }
}

impl ScanlineExrWriter {
    pub fn create(path: &Path, width: u32, height: u32, channels: &[&str]) -> Result<Self, String> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err("EXR dimensions must fit positive native i32 values".into());
        }
        let path = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| "EXR output path contains a NUL byte")?;
        let channels = c_channels(channels)?;
        let channel_ptrs: Vec<_> = channels.iter().map(|channel| channel.as_ptr()).collect();
        // SAFETY: path and channel pointers are valid NUL-terminated strings for this call;
        // dimensions and channel count were validated above.
        let writer = unsafe {
            planet_gen_exr_open(
                path.as_ptr(),
                width as i32,
                height as i32,
                channel_ptrs.as_ptr(),
                channel_ptrs.len() as i32,
            )
        };
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
            channels,
        })
    }

    pub fn write_scanline(&mut self, pixels: &[f32]) -> Result<(), String> {
        let expected = self
            .width
            .checked_mul(self.channels.len())
            .ok_or("EXR scanline channel count overflows usize")?;
        if pixels.len() != expected {
            return Err("EXR scanline has an unexpected channel count".into());
        }
        if self.next_row == self.height {
            return Err("all declared EXR scanlines were already written".into());
        }
        let channel_ptrs: Vec<_> = self
            .channels
            .iter()
            .map(|channel| channel.as_ptr())
            .collect();
        // SAFETY: writer is owned by self, pixels holds one interleaved row with the validated
        // width and channel count, and the channel pointers outlive this call.
        let written = unsafe {
            planet_gen_exr_write_scanline(
                self.writer,
                self.next_row as i32,
                pixels.as_ptr(),
                channel_ptrs.as_ptr(),
                channel_ptrs.len() as i32,
            )
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
        // SAFETY: writer is created by planet_gen_exr_open and is closed exactly once here.
        unsafe { planet_gen_exr_close(self.writer) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_named_float_zip_scanlines_atomically() {
        let path =
            std::env::temp_dir().join(format!("planet-gen-openexr-{}.exr", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut writer = AtomicScanlineExrWriter::create(&path, 2, 2, &["Y", "coverage"]).unwrap();
        writer.write_scanline(&[0.0, 0.25, 1.0, 0.75]).unwrap();
        writer.write_scanline(&[0.1, 0.2, 0.4, 0.5]).unwrap();
        writer.finish().unwrap();
        let image = exr::prelude::read_first_flat_layer_from_file(&path).unwrap();
        assert_eq!(
            image.layer_data.encoding.compression,
            exr::prelude::Compression::ZIP16
        );
        let names: Vec<_> = image
            .layer_data
            .channel_data
            .list
            .iter()
            .map(|channel| channel.name.to_string())
            .collect();
        assert_eq!(names, ["Y", "coverage"]);
        assert!(
            image
                .layer_data
                .channel_data
                .list
                .iter()
                .all(|channel| matches!(channel.sample_data, exr::image::FlatSamples::F32(_)))
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn incomplete_named_output_is_never_published() {
        let path = std::env::temp_dir().join(format!(
            "planet-gen-openexr-incomplete-{}.exr",
            std::process::id()
        ));
        let staging = path.with_extension("exr.part");
        let _ = std::fs::remove_file(&path);
        let mut writer = AtomicScanlineExrWriter::create(&path, 1, 2, &["Y"]).unwrap();
        writer.write_scanline(&[0.0]).unwrap();
        assert!(writer.finish().is_err());
        assert!(!path.exists());
        assert!(!staging.exists());
    }

    #[test]
    fn dropped_writer_closes_before_removing_the_partial_file() {
        let path = std::env::temp_dir().join(format!(
            "planet-gen-openexr-drop-{}.exr",
            std::process::id()
        ));
        let staging = path.with_extension("exr.part");
        let _ = std::fs::remove_file(&path);
        let writer = AtomicScanlineExrWriter::create(&path, 1, 1, &["Y"]).unwrap();
        assert!(staging.exists());
        drop(writer);
        assert!(!path.exists());
        assert!(!staging.exists());
    }

    #[test]
    fn failed_scanline_write_cleans_the_partial_on_drop() {
        let path = std::env::temp_dir().join(format!(
            "planet-gen-openexr-write-failure-{}.exr",
            std::process::id()
        ));
        let staging = path.with_extension("exr.part");
        let _ = std::fs::remove_file(&path);
        let mut writer = AtomicScanlineExrWriter::create(&path, 1, 1, &["Y"]).unwrap();
        assert!(writer.write_scanline(&[]).is_err());
        drop(writer);
        assert!(!path.exists());
        assert!(!staging.exists());
    }
}
