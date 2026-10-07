//! An in-memory Typst world: one main source, a few named files and the
//! embedded fonts. Nothing touches the disk, the network or the clock.

use std::sync::OnceLock;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};

const FONT_FILES: [&[u8]; 3] = [
    include_bytes!("../fonts/Geist-Regular.ttf"),
    include_bytes!("../fonts/Geist-Medium.ttf"),
    include_bytes!("../fonts/Geist-SemiBold.ttf"),
];

struct Shared {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| {
        let fonts: Vec<Font> = FONT_FILES
            .iter()
            .flat_map(|data| Font::iter(Bytes::new(*data)))
            .collect();
        Shared {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(FontBook::from_fonts(&fonts)),
            fonts,
        }
    })
}

fn file_id(path: &str) -> FileId {
    let vpath = VirtualPath::new(path).expect("static template paths are valid");
    FileId::new(RootedPath::new(VirtualRoot::Project, vpath))
}

pub(crate) struct MemoryWorld {
    main: Source,
    files: Vec<(FileId, Bytes)>,
}

impl MemoryWorld {
    pub(crate) fn new(template: &str, files: Vec<(&str, Vec<u8>)>) -> Self {
        Self {
            main: Source::new(file_id("/main.typ"), template.to_owned()),
            files: files
                .into_iter()
                .map(|(path, data)| (file_id(path), Bytes::new(data)))
                .collect(),
        }
    }

    fn not_found(id: FileId) -> FileError {
        FileError::NotFound(id.get().vpath().get_without_slash().into())
    }
}

impl World for MemoryWorld {
    fn library(&self) -> &LazyHash<Library> {
        &shared().library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &shared().book
    }

    fn main(&self) -> FileId {
        self.main.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(Self::not_found(id))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main.id() {
            return Ok(Bytes::from_string(self.main.clone()));
        }
        self.files
            .iter()
            .find(|(f, _)| *f == id)
            .map(|(_, data)| data.clone())
            .ok_or_else(|| Self::not_found(id))
    }

    fn font(&self, index: usize) -> Option<Font> {
        shared().fonts.get(index).cloned()
    }

    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}
