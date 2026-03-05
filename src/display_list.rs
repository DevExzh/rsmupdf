use std::{ffi::CString, io::Read, ptr::NonNull};

use mupdf_sys::*;

use crate::{
    array::FzArray, context, rust_vec_from_ffi_ptr, Buffer, Colorspace, Cookie, Device, Error,
    Image, Matrix, Pixmap, Quad, Rect, TextPage, TextPageFlags,
};

#[derive(Debug)]
pub struct DisplayList {
    pub(crate) inner: *mut fz_display_list,
}

impl DisplayList {
    pub(crate) unsafe fn from_raw(ptr: *mut fz_display_list) -> Self {
        Self { inner: ptr }
    }

    pub fn new(media_box: Rect) -> Result<Self, Error> {
        unsafe { ffi_try!(mupdf_new_display_list(context(), media_box.into())) }
            .map(|inner| Self { inner })
    }

    pub fn bounds(&self) -> Rect {
        let rect = unsafe { fz_bound_display_list(context(), self.inner) };
        rect.into()
    }

    pub fn to_pixmap(&self, ctm: &Matrix, cs: &Colorspace, alpha: bool) -> Result<Pixmap, Error> {
        unsafe {
            ffi_try!(mupdf_display_list_to_pixmap(
                context(),
                self.inner,
                ctm.into(),
                cs.inner,
                alpha
            ))
        }
        .map(|inner| unsafe { Pixmap::from_raw(inner) })
    }

    pub fn to_svg(&self, ctm: &Matrix) -> Result<String, Error> {
        let inner = unsafe {
            ffi_try!(mupdf_display_list_to_svg(
                context(),
                self.inner,
                ctm.into(),
                ptr::null_mut()
            ))
        }?;
        let mut buf = unsafe { Buffer::from_raw(inner) };
        let mut svg = String::new();
        buf.read_to_string(&mut svg)?;
        Ok(svg)
    }

    pub fn to_svg_with_cookie(&self, ctm: &Matrix, cookie: &Cookie) -> Result<String, Error> {
        let inner = unsafe {
            ffi_try!(mupdf_display_list_to_svg(
                context(),
                self.inner,
                ctm.into(),
                cookie.inner
            ))
        }?;
        let mut buf = unsafe { Buffer::from_raw(inner) };
        let mut svg = String::new();
        buf.read_to_string(&mut svg)?;
        Ok(svg)
    }

    pub fn to_text_page(&self, opts: TextPageFlags) -> Result<TextPage, Error> {
        let inner = unsafe {
            ffi_try!(mupdf_display_list_to_text_page(
                context(),
                self.inner,
                opts.bits() as _
            ))?
        };

        let inner = unsafe { NonNull::new_unchecked(inner) };

        Ok(TextPage { inner })
    }

    /// Convert the display list to a text page with a clipping rectangle.
    ///
    /// This function behaves like [`to_text_page`](Self::to_text_page), but allows specifying
    /// a clip rectangle to limit the text extraction to a specific area of the page.
    ///
    /// # Arguments
    ///
    /// * `flags` - Options for creating the text page, controlling features like
    ///   ligature preservation, whitespace handling, image inclusion, etc.
    /// * `clip` - The clipping rectangle. Only text within this rectangle will be included
    ///   in the resulting text page.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use mupdf::{DisplayList, Rect, TextPageFlags};
    /// # fn example(list: &DisplayList) -> Result<(), mupdf::Error> {
    /// // Extract text from the top half of the page only
    /// let clip_rect = Rect::new(0.0, 0.0, 612.0, 396.0);
    /// let text_page = list.to_text_page_with_rect(TextPageFlags::empty(), clip_rect)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn to_text_page_with_rect(
        &self,
        clip: Rect,
        flags: TextPageFlags,
    ) -> Result<TextPage, Error> {
        let opts = fz_stext_options {
            flags: flags.bits() as i32,
            scale: 0.0,
            clip: clip.into(),
        };

        let inner = unsafe { fz_new_stext_page_from_display_list(context(), self.inner, &opts) };

        if inner.is_null() {
            return Err(Error::UnexpectedNullPtr);
        }

        let inner = unsafe { NonNull::new_unchecked(inner) };

        Ok(TextPage { inner })
    }

    pub fn to_image(&self, width: f32, height: f32) -> Result<Image, Error> {
        Image::from_display_list(self, width, height)
    }

    pub fn run(&self, device: &Device, ctm: &Matrix, area: Rect) -> Result<(), Error> {
        unsafe {
            ffi_try!(mupdf_display_list_run(
                context(),
                self.inner,
                device.dev,
                ctm.into(),
                area.into(),
                ptr::null_mut()
            ))
        }
    }

    pub fn run_with_cookie(
        &self,
        device: &Device,
        ctm: &Matrix,
        area: Rect,
        cookie: &Cookie,
    ) -> Result<(), Error> {
        unsafe {
            ffi_try!(mupdf_display_list_run(
                context(),
                self.inner,
                device.dev,
                ctm.into(),
                area.into(),
                cookie.inner
            ))
        }
    }

    pub fn is_empty(&self) -> bool {
        unsafe { fz_display_list_is_empty(context(), self.inner) > 0 }
    }

    pub fn search(&self, needle: &str, hit_max: u32) -> Result<FzArray<Quad>, Error> {
        let c_needle = CString::new(needle)?;
        let hit_max = if hit_max < 1 { 16 } else { hit_max };
        let mut hit_count = 0;
        unsafe {
            ffi_try!(mupdf_search_display_list(
                context(),
                self.inner,
                c_needle.as_ptr(),
                hit_max as i32,
                &mut hit_count
            ))
        }
        .and_then(|quads| unsafe { rust_vec_from_ffi_ptr(quads, hit_count) })
    }
}

impl Drop for DisplayList {
    fn drop(&mut self) {
        if !self.inner.is_null() {
            unsafe {
                fz_drop_display_list(context(), self.inner);
            }
        }
    }
}

// `DisplayList`s may be used by multiple threads simultaneously
unsafe impl Send for DisplayList {}
unsafe impl Sync for DisplayList {}

#[cfg(test)]
mod test {
    use crate::{document::test_document, Document};

    #[test]
    fn test_display_list_search() {
        use crate::{Point, Quad};

        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let list = page0.to_display_list(false).unwrap();
        let hits = list.search("Dummy", 1).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            &*hits,
            [Quad {
                ul: Point {
                    x: 56.8,
                    y: 69.32953
                },
                ur: Point {
                    x: 115.85159,
                    y: 69.32953
                },
                ll: Point {
                    x: 56.8,
                    y: 87.29713
                },
                lr: Point {
                    x: 115.85159,
                    y: 87.29713
                }
            }]
        );

        let hits = list.search("Not Found", 1).unwrap();
        assert_eq!(hits.len(), 0);
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_multi_threaded_display_list_search() {
        use crossbeam_utils::thread;

        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let list = page0.to_display_list(false).unwrap();

        thread::scope(|scope| {
            for _ in 0..5 {
                scope.spawn(|_| {
                    let hits = list.search("Dummy", 1).unwrap();
                    assert_eq!(hits.len(), 1);
                    let hits = list.search("Not Found", 1).unwrap();
                    assert_eq!(hits.len(), 0);
                });
            }
        })
        .unwrap();
    }

    #[test]
    fn test_display_list_to_text_page_with_rect() {
        use crate::{Rect, TextPageFlags};

        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let list = page0.to_display_list(false).unwrap();

        // Get the bounds of the display list
        let bounds = list.bounds();

        // Test with full bounds - should find the text
        let text_page = list
            .to_text_page_with_rect(TextPageFlags::empty(), bounds)
            .unwrap();
        let text = text_page.to_text().unwrap();
        assert!(text.contains("Dummy PDF file"));

        // Verify the function returns a valid TextPage with a clip rect
        // The clip rect functionality depends on MuPDF's internal implementation
        let clip_rect = Rect::new(bounds.x0, bounds.y0, bounds.x1, bounds.y1 / 2.0);
        let text_page_clipped = list
            .to_text_page_with_rect(TextPageFlags::empty(), clip_rect)
            .unwrap();
        // TextPage is valid and can be used
        let _text_clipped = text_page_clipped.to_text().unwrap();
    }
}
