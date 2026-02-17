use std::{
    convert::TryInto,
    ffi::{c_int, c_void, CString},
    io::Read,
    marker::PhantomData,
    ptr::{self, NonNull},
    slice,
};

use bitflags::bitflags;
use mupdf_sys::*;

use crate::{
    context, from_enum, rust_slice_to_ffi_ptr, unsafe_impl_ffi_wrapper, Buffer, Error,
    FFIWrapper, Font, Image, Matrix, Point, Quad, Rect, WriteMode,
};
use crate::{output::Output, FFIAnalogue};

bitflags! {
    /// Options for creating a pixmap and draw device.
    pub struct TextPageFlags: u32 {
        const PRESERVE_LIGATURES = FZ_STEXT_PRESERVE_LIGATURES as _;
        const PRESERVE_WHITESPACE = FZ_STEXT_PRESERVE_WHITESPACE as _;
        const PRESERVE_IMAGES = FZ_STEXT_PRESERVE_IMAGES as _;
        const INHIBIT_SPACES = FZ_STEXT_INHIBIT_SPACES as _;
        const DEHYPHENATE = FZ_STEXT_DEHYPHENATE as _;
        const PRESERVE_SPANS = FZ_STEXT_PRESERVE_SPANS as _;
        const CLIP = FZ_STEXT_CLIP as _;
        const USE_CID_FOR_UNKNOWN_UNICODE = FZ_STEXT_USE_CID_FOR_UNKNOWN_UNICODE as _;
        const COLLECT_STRUCTURE = FZ_STEXT_COLLECT_STRUCTURE as _;
        const ACCURATE_BBOXES = FZ_STEXT_ACCURATE_BBOXES as _;
        const COLLECT_VECTORS = FZ_STEXT_COLLECT_VECTORS as _;
        const IGNORE_ACTUALTEXT = FZ_STEXT_IGNORE_ACTUALTEXT as _;
        const SEGMENT = FZ_STEXT_SEGMENT as _;
        const PARAGRAPH_BREAK = FZ_STEXT_PARAGRAPH_BREAK as _;
        const TABLE_HUNT = FZ_STEXT_TABLE_HUNT as _;
        const COLLECT_STYLES = FZ_STEXT_COLLECT_STYLES as _;
        const USE_GID_FOR_UNKNOWN_UNICODE = FZ_STEXT_USE_GID_FOR_UNKNOWN_UNICODE as _;
        const ACCURATE_ASCENDERS = FZ_STEXT_ACCURATE_ASCENDERS as _;
        const ACCURATE_SIDE_BEARINGS = FZ_STEXT_ACCURATE_SIDE_BEARINGS as _;
    }
}

bitflags! {
    /// Font style flags derived from font properties.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FontFlags: u32 {
        const SUPERSCRIPT = 1;
        const ITALIC = 2;
        const SERIFED = 4;
        const MONOSPACED = 8;
        const BOLD = 16;
    }
}

bitflags! {
    /// Character-level flags from fz_stext_char.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct CharFlags: u16 {
        const STRIKEOUT = 1;
        const UNDERLINE = 2;
        const SYNTHETIC = 4;
    }
}

/// Represents a text span - a sequence of characters with uniform styling
#[derive(Debug, Clone)]
pub struct TextSpan {
    pub text: String,
    pub font_name: String,
    pub font_size: f32,
    pub font_flags: FontFlags,
    pub char_flags: CharFlags,
    pub color: u32,
    pub alpha: u8,
    pub origin: Point,
    pub bbox: Rect,
    pub ascender: f32,
    pub descender: f32,
    pub bidi_level: u16,
}

/// Represents a word in the text with its bounding box and metadata
#[derive(Debug, Clone)]
pub struct TextWord {
    pub text: String,
    pub bbox: Rect,
    pub block_num: usize,
    pub line_num: usize,
    pub word_num: usize,
}

/// Check if a character is a word delimiter
fn is_word_delimiter(c: char, custom_delimiters: Option<&str>) -> bool {
    if let Some(delimiters) = custom_delimiters {
        delimiters.contains(c)
    } else {
        // Default word delimiters: whitespace and common punctuation
        c.is_whitespace()
            || matches!(
                c,
                ',' | '.' | ';' | ':' | '!' | '?' | '-' | '(' | ')' | '[' | ']' | '{' | '}'
            )
    }
}

/// A text page is a list of blocks, together with an overall bounding box
#[derive(Debug)]
pub struct TextPage {
    pub(crate) inner: NonNull<fz_stext_page>,
}

unsafe_impl_ffi_wrapper!(TextPage, fz_stext_page, fz_drop_stext_page);

impl TextPage {
    pub fn to_html(&self, id: i32, full: bool) -> Result<String, Error> {
        let mut buf = Buffer::with_capacity(8192);

        let out = Output::from_buffer(&buf);
        if full {
            unsafe {
                ffi_try!(mupdf_print_stext_header_as_html(
                    context(),
                    out.inner.as_ptr()
                ))?
            };
        }
        unsafe {
            ffi_try!(mupdf_print_stext_page_as_html(
                context(),
                out.inner.as_ptr(),
                self.inner.as_ptr(),
                id
            ))?
        };
        if full {
            unsafe {
                ffi_try!(mupdf_print_stext_trailer_as_html(
                    context(),
                    out.inner.as_ptr()
                ))?
            };
        }
        drop(out);

        let mut res = String::new();
        buf.read_to_string(&mut res)?;
        Ok(res)
    }

    pub fn to_xhtml(&self, id: i32) -> Result<String, Error> {
        let mut buf = Buffer::with_capacity(8192);

        let out = Output::from_buffer(&buf);
        unsafe {
            ffi_try!(mupdf_print_stext_header_as_xhtml(
                context(),
                out.inner.as_ptr()
            ))?;
            ffi_try!(mupdf_print_stext_page_as_xhtml(
                context(),
                out.inner.as_ptr(),
                self.inner.as_ptr(),
                id
            ))?;
            ffi_try!(mupdf_print_stext_trailer_as_html(
                context(),
                out.inner.as_ptr()
            ))?;
        }
        drop(out);

        let mut res = String::new();
        buf.read_to_string(&mut res)?;
        Ok(res)
    }

    pub fn to_xml(&self, id: i32) -> Result<String, Error> {
        let mut buf = Buffer::with_capacity(8192);

        let out = Output::from_buffer(&buf);
        unsafe {
            ffi_try!(mupdf_print_stext_page_as_xml(
                context(),
                out.inner.as_ptr(),
                self.inner.as_ptr(),
                id
            ))?
        };
        drop(out);

        let mut res = String::new();
        buf.read_to_string(&mut res)?;
        Ok(res)
    }

    pub fn to_text(&self) -> Result<String, Error> {
        let mut buf = Buffer::with_capacity(8192);

        let out = Output::from_buffer(&buf);
        unsafe {
            ffi_try!(mupdf_print_stext_page_as_text(
                context(),
                out.inner.as_ptr(),
                self.inner.as_ptr()
            ))?
        };
        drop(out);

        let mut res = String::new();
        buf.read_to_string(&mut res)?;
        Ok(res)
    }

    pub fn to_json(&self, scale: f32) -> Result<String, Error> {
        let mut buf = Buffer::with_capacity(8192);

        let out = Output::from_buffer(&buf);
        unsafe {
            ffi_try!(mupdf_print_stext_page_as_json(
                context(),
                out.inner.as_ptr(),
                self.inner.as_ptr(),
                scale
            ))?
        };
        drop(out);

        let mut res = String::new();
        buf.read_to_string(&mut res)?;
        Ok(res)
    }

    pub fn blocks(&self) -> TextBlockIter<'_> {
        TextBlockIter {
            next: unsafe { (*self.as_ptr().cast_mut()).first_block },
            _marker: PhantomData,
        }
    }

    pub fn search(&self, needle: &str) -> Result<Vec<Quad>, Error> {
        let mut vec = Vec::new();
        self.search_cb(needle, &mut vec, |v, quads| {
            v.extend(quads.iter().cloned());
            SearchHitResponse::ContinueSearch
        })?;
        Ok(vec)
    }

    /// Search through the page, finding all instances of `needle` and processing them through
    /// `cb`.
    /// Note that the `&[Quad]` given to `cb` in its invocation lives only during the time that
    /// `cb` is being evaluated. That means the following won't work or compile:
    ///
    /// ```compile_fail
    /// # use mupdf::{TextPage, Quad, text_page::SearchHitResponse};
    /// # let text_page: TextPage = todo!();
    /// let mut quads: Vec<&Quad> = Vec::new();
    /// text_page.search_cb("search term", &mut quads, |v, quads: &[Quad]| {
    ///     v.extend(quads);
    ///     SearchHitResponse::ContinueSearch
    /// }).unwrap();
    /// ```
    ///
    /// But the following will:
    /// ```no_run
    /// # use mupdf::{TextPage, Quad, text_page::SearchHitResponse};
    /// # let text_page: TextPage = todo!();
    /// let mut quads: Vec<Quad> = Vec::new();
    /// text_page.search_cb("search term", &mut quads, |v, quads: &[Quad]| {
    ///     v.extend(quads.iter().cloned());
    ///     SearchHitResponse::ContinueSearch
    /// }).unwrap();
    /// ```
    pub fn search_cb<T, F>(&self, needle: &str, data: &mut T, cb: F) -> Result<u32, Error>
    where
        T: ?Sized,
        F: Fn(&mut T, &[Quad]) -> SearchHitResponse,
    {
        // This struct allows us to wrap both the callback that the user gave us and the data so
        // that we can pass it into the ffi callback nicely
        struct FnWithData<'parent, T: ?Sized, F>
        where
            F: Fn(&mut T, &[Quad]) -> SearchHitResponse,
        {
            data: &'parent mut T,
            f: F,
        }

        let mut opaque = FnWithData { data, f: cb };

        // And then here's the `fn` that we'll pass in - it has to be an fn, not capturing context,
        // because it needs to be unsafe extern "C". to be used with FFI.
        unsafe extern "C" fn ffi_cb<T, F>(
            _ctx: *mut fz_context,
            data: *mut c_void,
            num_quads: c_int,
            hit_bbox: *mut fz_quad,
        ) -> c_int
        where
            T: ?Sized,
            F: Fn(&mut T, &[Quad]) -> SearchHitResponse,
            Quad: FFIAnalogue<FFIType = fz_quad>,
        {
            // This is upheld by our `FFIAnalogue` bound above
            let quad_ptr = hit_bbox.cast::<Quad>();
            let Some(nn) = NonNull::new(quad_ptr) else {
                return SearchHitResponse::ContinueSearch as c_int;
            };

            // This guarantee is upheld by mupdf - they're giving us a pointer to the same type we
            // gave them.
            let data = data.cast::<FnWithData<'_, T, F>>();

            // But if they like gave us a -1 for number of results or whatever, give up on
            // decoding.
            let Ok(len) = usize::try_from(num_quads) else {
                return SearchHitResponse::ContinueSearch as c_int;
            };

            // SAFETY: We've ensure nn is not null, and we're trusting the FFI layer for the other
            // invariants (about actually holding the data, etc)
            let slice = unsafe { slice::from_raw_parts_mut(nn.as_ptr(), len) };

            // Get the function and the data
            // SAFETY: Trusting that the FFI layer actually gave us this ptr
            let f = unsafe { &(*data).f };
            // SAFETY: Trusting that the FFI layer actually gave us this ptr
            let data = unsafe { &mut (*data).data };

            // And call the function with the data
            f(data, slice) as c_int
        }

        let c_needle = CString::new(needle)?;
        unsafe {
            ffi_try!(mupdf_search_stext_page_cb(
                context(),
                self.as_ptr().cast_mut(),
                c_needle.as_ptr(),
                Some(ffi_cb::<T, F>),
                &raw mut opaque as *mut c_void
            ))
        }
        .map(|count| count as u32)
    }

    pub fn highlight_selection(
        &mut self,
        a: Point,
        b: Point,
        quads: &[Quad],
    ) -> Result<i32, Error> {
        let (ptr, len): (*const fz_quad, _) = rust_slice_to_ffi_ptr(quads)?;

        unsafe {
            ffi_try!(mupdf_highlight_selection(
                context(),
                self.as_mut_ptr(),
                a.into(),
                b.into(),
                ptr as *mut fz_quad,
                len
            ))
        }
    }

    /// Extract words from the text page with optional custom delimiters
    pub fn extract_words(&self, delimiters: Option<&str>) -> Vec<TextWord> {
        let mut words = Vec::new();

        for (block_num, block) in self.blocks().enumerate() {
            if block.r#type() != TextBlockType::Text {
                continue;
            }

            for (line_num, line) in block.lines().enumerate() {
                let mut word_num = 0usize;
                let mut current_word = String::new();
                let mut word_bbox = Rect::new(0.0, 0.0, 0.0, 0.0);

                for ch in line.chars() {
                    if let Some(c) = ch.char() {
                        if is_word_delimiter(c, delimiters) {
                            if !current_word.is_empty() && !word_bbox.is_empty() {
                                words.push(TextWord {
                                    text: current_word.clone(),
                                    bbox: word_bbox,
                                    block_num,
                                    line_num,
                                    word_num,
                                });
                                word_num += 1;
                                current_word.clear();
                                word_bbox = Rect::new(0.0, 0.0, 0.0, 0.0);
                            }
                        } else {
                            current_word.push(c);
                            let char_rect = Rect::from(ch.quad());
                            word_bbox = if word_bbox.is_empty() {
                                char_rect
                            } else {
                                word_bbox.union(&char_rect)
                            };
                        }
                    }
                }

                if !current_word.is_empty() && !word_bbox.is_empty() {
                    words.push(TextWord {
                        text: current_word,
                        bbox: word_bbox,
                        block_num,
                        line_num,
                        word_num,
                    });
                }
            }
        }

        words
    }
}

#[repr(i32)]
pub enum SearchHitResponse {
    ContinueSearch = 0,
    AbortSearch = 1,
}

from_enum! { c_int => c_int,
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum TextBlockType {
        Text = FZ_STEXT_BLOCK_TEXT,
        Image = FZ_STEXT_BLOCK_IMAGE,
        Struct = FZ_STEXT_BLOCK_STRUCT,
        Vector = FZ_STEXT_BLOCK_VECTOR,
        Grid = FZ_STEXT_BLOCK_GRID,
    }
}

/// A text block is a list of lines of text (typically a paragraph), or an image.
pub struct TextBlock<'a> {
    inner: &'a fz_stext_block,
}

impl TextBlock<'_> {
    pub fn r#type(&self) -> TextBlockType {
        self.inner.type_.try_into().unwrap()
    }

    pub fn bounds(&self) -> Rect {
        self.inner.bbox.into()
    }

    pub fn lines(&self) -> TextLineIter<'_> {
        unsafe {
            if self.inner.type_ == FZ_STEXT_BLOCK_TEXT as c_int {
                return TextLineIter {
                    next: self.inner.u.t.first_line,
                    _marker: PhantomData,
                };
            }
        }
        TextLineIter {
            next: ptr::null_mut(),
            _marker: PhantomData,
        }
    }

    pub fn ctm(&self) -> Option<Matrix> {
        unsafe {
            if self.inner.type_ == FZ_STEXT_BLOCK_IMAGE as i32 {
                return Some(self.inner.u.i.transform.into());
            }
        }
        None
    }

    pub fn image(&self) -> Option<Image> {
        unsafe {
            if self.inner.type_ == FZ_STEXT_BLOCK_IMAGE as i32 {
                let inner = self.inner.u.i.image;
                fz_keep_image(context(), inner);
                return Some(Image::from_raw(inner));
            }
        }
        None
    }
}

#[derive(Debug)]
pub struct TextBlockIter<'a> {
    next: *mut fz_stext_block,
    _marker: PhantomData<TextBlock<'a>>,
}

impl<'a> Iterator for TextBlockIter<'a> {
    type Item = TextBlock<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next.is_null() {
            return None;
        }
        let node = unsafe { &*self.next };
        self.next = node.next;
        Some(TextBlock { inner: node })
    }
}

/// A text line is a list of characters that share a common baseline.
#[derive(Debug)]
pub struct TextLine<'a> {
    inner: &'a fz_stext_line,
}

impl TextLine<'_> {
    pub fn bounds(&self) -> Rect {
        self.inner.bbox.into()
    }

    pub fn wmode(&self) -> WriteMode {
        (self.inner.wmode as u32).try_into().unwrap()
    }

    pub fn chars(&self) -> TextCharIter<'_> {
        TextCharIter {
            next: self.inner.first_char,
            _marker: PhantomData,
        }
    }

    /// Extract text spans from this line - groups of characters with uniform styling
    pub fn extract_spans(&self) -> Vec<TextSpan> {
        let mut spans = Vec::new();
        let mut current_span: Option<TextSpan> = None;

        for ch in self.chars() {
            let font = ch.font();
            let font_name = font.as_ref().map(|f| f.name().to_string()).unwrap_or_default();
            let font_size = ch.size();
            let font_flags = ch.font_flags();
            let char_flags = ch.char_flags();
            let color = ch.color();
            let alpha = ch.alpha();
            let origin = ch.origin();
            let bidi_level = ch.bidi_level();

            let ascender = font.as_ref().map(|f| f.ascender()).unwrap_or(0.9);
            let descender = font.as_ref().map(|f| f.descender()).unwrap_or(-0.1);

            // Check if we need to start a new span
            let need_new_span = if let Some(ref span) = current_span {
                span.font_name != font_name
                    || (span.font_size - font_size).abs() > 0.001
                    || span.font_flags != font_flags
                    || span.char_flags != char_flags
                    || span.color != color
                    || span.bidi_level != bidi_level
            } else {
                true
            };

            if need_new_span {
                // Save the previous span if it exists
                if let Some(span) = current_span.take() {
                    spans.push(span);
                }

                // Start a new span
                current_span = Some(TextSpan {
                    text: String::new(),
                    font_name,
                    font_size,
                    font_flags,
                    char_flags,
                    color,
                    alpha,
                    origin,
                    bbox: Rect::from(ch.quad()),
                    ascender,
                    descender,
                    bidi_level,
                });
            }

            // Add character to current span
            if let Some(ref mut span) = current_span {
                if let Some(c) = ch.char() {
                    span.text.push(c);
                }
                // Update bbox to include this character
                let char_rect = Rect::from(ch.quad());
                span.bbox = span.bbox.union(&char_rect);
            }
        }

        // Don't forget the last span
        if let Some(span) = current_span {
            spans.push(span);
        }

        spans
    }
}

#[derive(Debug)]
pub struct TextLineIter<'a> {
    next: *mut fz_stext_line,
    _marker: PhantomData<TextLine<'a>>,
}

impl<'a> Iterator for TextLineIter<'a> {
    type Item = TextLine<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next.is_null() {
            return None;
        }
        let node = unsafe { &*self.next };
        self.next = node.next;
        Some(TextLine { inner: node })
    }
}

/// A text char is a unicode character, the style in which is appears,
/// and the point at which it is positioned.
#[derive(Debug)]
pub struct TextChar<'a> {
    inner: &'a fz_stext_char,
}

impl TextChar<'_> {
    pub fn char(&self) -> Option<char> {
        std::char::from_u32(self.inner.c as u32)
    }

    pub fn origin(&self) -> Point {
        self.inner.origin.into()
    }

    pub fn size(&self) -> f32 {
        self.inner.size
    }

    pub fn quad(&self) -> Quad {
        self.inner.quad.into()
    }

    pub fn font(&self) -> Option<Font> {
        if self.inner.font.is_null() {
            return None;
        }
        unsafe {
            fz_keep_font(context(), self.inner.font);
            Some(Font::from_raw(self.inner.font))
        }
    }

    pub fn color(&self) -> u32 {
        self.inner.argb & 0xFFFFFF
    }

    pub fn alpha(&self) -> u8 {
        ((self.inner.argb >> 24) & 0xFF) as u8
    }

    pub fn argb(&self) -> u32 {
        self.inner.argb
    }

    pub fn bidi_level(&self) -> u16 {
        self.inner.bidi
    }

    pub fn char_flags(&self) -> CharFlags {
        CharFlags::from_bits_truncate(self.inner.flags)
    }

    pub fn font_flags(&self) -> FontFlags {
        let mut flags = FontFlags::empty();
        if let Some(font) = self.font() {
            if font.is_bold() {
                flags |= FontFlags::BOLD;
            }
            if font.is_italic() {
                flags |= FontFlags::ITALIC;
            }
            if font.is_serif() {
                flags |= FontFlags::SERIFED;
            }
            if font.is_monospaced() {
                flags |= FontFlags::MONOSPACED;
            }
        }
        flags
    }
}

#[derive(Debug)]
pub struct TextCharIter<'a> {
    next: *mut fz_stext_char,
    _marker: PhantomData<TextChar<'a>>,
}

impl<'a> Iterator for TextCharIter<'a> {
    type Item = TextChar<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next.is_null() {
            return None;
        }
        let node = unsafe { &*self.next };
        self.next = node.next;
        Some(TextChar { inner: node })
    }
}

#[cfg(test)]
mod test {
    use crate::{document::test_document, text_page::SearchHitResponse, Document, TextPageFlags};

    #[test]
    fn test_page_to_html() {
        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let text_page = page0.to_text_page(TextPageFlags::empty()).unwrap();

        let html = text_page.to_html(0, false).unwrap();
        assert!(!html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("Dummy PDF file"));

        let html = text_page.to_html(0, true).unwrap();
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("Dummy PDF file"));
    }

    #[test]
    fn test_page_to_xhtml() {
        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let text_page = page0.to_text_page(TextPageFlags::empty()).unwrap();

        let xhtml = text_page.to_xhtml(0).unwrap();
        assert!(xhtml.starts_with("<?xml "));
        assert!(xhtml.contains("Dummy PDF file"));
    }

    #[test]
    fn test_page_to_xml() {
        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let text_page = page0.to_text_page(TextPageFlags::empty()).unwrap();
        let xml = text_page.to_xml(0).unwrap();
        assert!(xml.contains("Dummy PDF file"));
    }

    #[test]
    fn test_page_to_text() {
        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let text_page = page0.to_text_page(TextPageFlags::empty()).unwrap();
        let text = text_page.to_text().unwrap();
        assert_eq!(text, "Dummy PDF file\n\n");
    }

    #[test]
    fn test_text_page_search() {
        use crate::{Point, Quad};

        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let text_page = page0.to_text_page(TextPageFlags::empty()).unwrap();
        let hits = text_page.search("Dummy").unwrap();
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

        let hits = text_page.search("Not Found").unwrap();
        assert_eq!(hits.len(), 0);
    }

    #[test]
    fn test_text_page_cb_search() {
        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let text_page = page0.to_text_page(TextPageFlags::empty()).unwrap();
        let mut sum_x = 0.0;
        let num_hits = text_page
            .search_cb("Dummy", &mut sum_x, |acc, hits| {
                for q in hits {
                    *acc += q.ul.x + q.ur.x + q.ll.x + q.lr.x;
                }
                SearchHitResponse::ContinueSearch
            })
            .unwrap();
        assert_eq!(num_hits, 1);
        assert_eq!(sum_x, 56.8 + 115.85159 + 56.8 + 115.85159);

        let hits = text_page.search("Not Found").unwrap();
        assert_eq!(hits.len(), 0);
    }

    #[test]
    fn test_extract_words() {
        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let text_page = page0.to_text_page(TextPageFlags::empty()).unwrap();
        let words = text_page.extract_words(None);
        
        assert!(!words.is_empty());
        assert!(words.iter().any(|w| w.text == "Dummy"));
        assert!(words.iter().any(|w| w.text == "PDF"));
        assert!(words.iter().any(|w| w.text == "file"));
    }

    #[test]
    fn test_extract_spans() {
        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let text_page = page0.to_text_page(TextPageFlags::empty()).unwrap();
        
        for block in text_page.blocks() {
            for line in block.lines() {
                let spans = line.extract_spans();
                assert!(!spans.is_empty());
                for span in &spans {
                    assert!(!span.text.is_empty());
                    assert!(span.font_size > 0.0);
                }
            }
        }
    }

    #[test]
    fn test_char_font_properties() {
        let doc = test_document!("..", "files/dummy.pdf").unwrap();
        let page0 = doc.load_page(0).unwrap();
        let text_page = page0.to_text_page(TextPageFlags::empty()).unwrap();
        
        for block in text_page.blocks() {
            for line in block.lines() {
                for ch in line.chars() {
                    if let Some(font) = ch.font() {
                        assert!(!font.name().is_empty());
                    }
                    assert!(ch.size() > 0.0);
                    let _font_flags = ch.font_flags();
                    let _char_flags = ch.char_flags();
                    let _color = ch.color();
                    let _alpha = ch.alpha();
                }
            }
        }
    }
}
