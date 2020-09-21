use alloc::vec::Vec;
use core::fmt::Debug;
use core::{fmt, slice, str};

use crate::endian::{self, Endianness};
use crate::macho;
use crate::pod::Pod;
use crate::read::util::StringTable;
use crate::read::{
    self, ObjectSymbol, ObjectSymbolTable, ReadError, Result, SectionIndex, SectionKind,
    SymbolFlags, SymbolIndex, SymbolKind, SymbolMap, SymbolMapEntry, SymbolScope, SymbolSection,
};

use super::{MachHeader, MachOFile};

/// A table of symbol entries in a Mach-O file.
///
/// Also includes the string table used for the symbol names.
#[derive(Debug, Clone, Copy)]
pub struct SymbolTable<'data, Mach: MachHeader> {
    symbols: &'data [Mach::Nlist],
    strings: StringTable<'data>,
}

impl<'data, Mach: MachHeader> Default for SymbolTable<'data, Mach> {
    fn default() -> Self {
        SymbolTable {
            symbols: &[],
            strings: Default::default(),
        }
    }
}

impl<'data, Mach: MachHeader> SymbolTable<'data, Mach> {
    #[inline]
    pub(super) fn new(symbols: &'data [Mach::Nlist], strings: StringTable<'data>) -> Self {
        SymbolTable { symbols, strings }
    }

    /// Return the string table used for the symbol names.
    #[inline]
    pub fn strings(&self) -> StringTable<'data> {
        self.strings
    }

    /// Iterate over the symbols.
    #[inline]
    pub fn iter(&self) -> slice::Iter<'data, Mach::Nlist> {
        self.symbols.iter()
    }

    /// Return true if the symbol table is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// The number of symbols.
    #[inline]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Return the symbol at the given index.
    pub fn symbol(&self, index: usize) -> Result<&'data Mach::Nlist> {
        self.symbols
            .get(index)
            .read_error("Invalid Mach-O symbol index")
    }

    /// Construct a map from addresses to a user-defined map entry.
    pub fn map<Entry: SymbolMapEntry, F: Fn(&'data Mach::Nlist) -> Option<Entry>>(
        &self,
        f: F,
    ) -> SymbolMap<Entry> {
        let mut symbols = Vec::with_capacity(self.symbols.len());
        for nlist in self.symbols {
            if !nlist.is_definition() {
                continue;
            }
            if let Some(entry) = f(nlist) {
                symbols.push(entry);
            }
        }
        SymbolMap::new(symbols)
    }
}

/// An iterator over the symbols of a `MachOFile32`.
pub type MachOSymbolTable32<'data, 'file, Endian = Endianness> =
    MachOSymbolTable<'data, 'file, macho::MachHeader32<Endian>>;
/// An iterator over the symbols of a `MachOFile64`.
pub type MachOSymbolTable64<'data, 'file, Endian = Endianness> =
    MachOSymbolTable<'data, 'file, macho::MachHeader64<Endian>>;

/// A symbol table of a `MachOFile`.
#[derive(Debug, Clone, Copy)]
pub struct MachOSymbolTable<'data, 'file, Mach: MachHeader> {
    pub(super) file: &'file MachOFile<'data, Mach>,
}

impl<'data, 'file, Mach: MachHeader> read::private::Sealed
    for MachOSymbolTable<'data, 'file, Mach>
{
}

impl<'data, 'file, Mach: MachHeader> ObjectSymbolTable<'data>
    for MachOSymbolTable<'data, 'file, Mach>
{
    type Symbol = MachOSymbol<'data, 'file, Mach>;
    type SymbolIterator = MachOSymbolIterator<'data, 'file, Mach>;

    fn symbols(&self) -> Self::SymbolIterator {
        MachOSymbolIterator {
            file: self.file,
            index: 0,
        }
    }

    fn symbol_by_index(&self, index: SymbolIndex) -> Result<Self::Symbol> {
        let nlist = self.file.symbols.symbol(index.0)?;
        MachOSymbol::new(self.file, index, nlist).read_error("Unsupported Mach-O symbol index")
    }
}

/// An iterator over the symbols of a `MachOFile32`.
pub type MachOSymbolIterator32<'data, 'file, Endian = Endianness> =
    MachOSymbolIterator<'data, 'file, macho::MachHeader32<Endian>>;
/// An iterator over the symbols of a `MachOFile64`.
pub type MachOSymbolIterator64<'data, 'file, Endian = Endianness> =
    MachOSymbolIterator<'data, 'file, macho::MachHeader64<Endian>>;

/// An iterator over the symbols of a `MachOFile`.
pub struct MachOSymbolIterator<'data, 'file, Mach: MachHeader> {
    pub(super) file: &'file MachOFile<'data, Mach>,
    pub(super) index: usize,
}

impl<'data, 'file, Mach: MachHeader> fmt::Debug for MachOSymbolIterator<'data, 'file, Mach> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MachOSymbolIterator").finish()
    }
}

impl<'data, 'file, Mach: MachHeader> Iterator for MachOSymbolIterator<'data, 'file, Mach> {
    type Item = MachOSymbol<'data, 'file, Mach>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let index = self.index;
            let nlist = self.file.symbols.symbols.get(index)?;
            self.index += 1;
            if let Some(symbol) = MachOSymbol::new(self.file, SymbolIndex(index), nlist) {
                return Some(symbol);
            }
        }
    }
}

/// A symbol of a `MachOFile32`.
pub type MachOSymbol32<'data, 'file, Endian = Endianness> =
    MachOSymbol<'data, 'file, macho::MachHeader32<Endian>>;
/// A symbol of a `MachOFile64`.
pub type MachOSymbol64<'data, 'file, Endian = Endianness> =
    MachOSymbol<'data, 'file, macho::MachHeader64<Endian>>;

/// A symbol of a `MachOFile`.
#[derive(Debug, Clone, Copy)]
pub struct MachOSymbol<'data, 'file, Mach: MachHeader> {
    file: &'file MachOFile<'data, Mach>,
    index: SymbolIndex,
    nlist: &'data Mach::Nlist,
}

impl<'data, 'file, Mach: MachHeader> MachOSymbol<'data, 'file, Mach> {
    pub(super) fn new(
        file: &'file MachOFile<'data, Mach>,
        index: SymbolIndex,
        nlist: &'data Mach::Nlist,
    ) -> Option<Self> {
        if nlist.n_type() & macho::N_STAB != 0 {
            return None;
        }
        Some(MachOSymbol { file, index, nlist })
    }
}

impl<'data, 'file, Mach: MachHeader> read::private::Sealed for MachOSymbol<'data, 'file, Mach> {}

impl<'data, 'file, Mach: MachHeader> ObjectSymbol<'data> for MachOSymbol<'data, 'file, Mach> {
    #[inline]
    fn index(&self) -> SymbolIndex {
        self.index
    }

    fn name(&self) -> Result<&'data str> {
        let name = self
            .nlist
            .name(self.file.endian, self.file.symbols.strings)?;
        str::from_utf8(name)
            .ok()
            .read_error("Non UTF-8 Mach-O symbol name")
    }

    #[inline]
    fn address(&self) -> u64 {
        self.nlist.n_value(self.file.endian).into()
    }

    #[inline]
    fn size(&self) -> u64 {
        0
    }

    fn kind(&self) -> SymbolKind {
        self.section()
            .index()
            .and_then(|index| self.file.section_internal(index).ok())
            .map(|section| match section.kind {
                SectionKind::Text => SymbolKind::Text,
                SectionKind::Data
                | SectionKind::ReadOnlyData
                | SectionKind::ReadOnlyString
                | SectionKind::UninitializedData
                | SectionKind::Common => SymbolKind::Data,
                SectionKind::Tls | SectionKind::UninitializedTls | SectionKind::TlsVariables => {
                    SymbolKind::Tls
                }
                _ => SymbolKind::Unknown,
            })
            .unwrap_or(SymbolKind::Unknown)
    }

    fn section(&self) -> SymbolSection {
        match self.nlist.n_type() & macho::N_TYPE {
            macho::N_UNDF => SymbolSection::Undefined,
            macho::N_ABS => SymbolSection::Absolute,
            macho::N_SECT => {
                let n_sect = self.nlist.n_sect();
                if n_sect != 0 {
                    SymbolSection::Section(SectionIndex(n_sect as usize))
                } else {
                    SymbolSection::Unknown
                }
            }
            _ => SymbolSection::Unknown,
        }
    }

    #[inline]
    fn is_undefined(&self) -> bool {
        self.nlist.n_type() & macho::N_TYPE == macho::N_UNDF
    }

    #[inline]
    fn is_definition(&self) -> bool {
        self.nlist.is_definition()
    }

    #[inline]
    fn is_common(&self) -> bool {
        // Mach-O common symbols are based on section, not symbol
        false
    }

    #[inline]
    fn is_weak(&self) -> bool {
        self.nlist.n_desc(self.file.endian) & (macho::N_WEAK_REF | macho::N_WEAK_DEF) != 0
    }

    fn scope(&self) -> SymbolScope {
        let n_type = self.nlist.n_type();
        if n_type & macho::N_TYPE == macho::N_UNDF {
            SymbolScope::Unknown
        } else if n_type & macho::N_EXT == 0 {
            SymbolScope::Compilation
        } else if n_type & macho::N_PEXT != 0 {
            SymbolScope::Linkage
        } else {
            SymbolScope::Dynamic
        }
    }

    #[inline]
    fn is_global(&self) -> bool {
        self.scope() != SymbolScope::Compilation
    }

    #[inline]
    fn is_local(&self) -> bool {
        self.scope() == SymbolScope::Compilation
    }

    #[inline]
    fn flags(&self) -> SymbolFlags<SectionIndex> {
        let n_desc = self.nlist.n_desc(self.file.endian);
        SymbolFlags::MachO { n_desc }
    }
}

/// A trait for generic access to `Nlist32` and `Nlist64`.
#[allow(missing_docs)]
pub trait Nlist: Debug + Pod {
    type Word: Into<u64>;
    type Endian: endian::Endian;

    fn n_strx(&self, endian: Self::Endian) -> u32;
    fn n_type(&self) -> u8;
    fn n_sect(&self) -> u8;
    fn n_desc(&self, endian: Self::Endian) -> u16;
    fn n_value(&self, endian: Self::Endian) -> Self::Word;

    fn name<'data>(
        &self,
        endian: Self::Endian,
        strings: StringTable<'data>,
    ) -> Result<&'data [u8]> {
        strings
            .get(self.n_strx(endian))
            .read_error("Invalid Mach-O symbol name offset")
    }

    /// Return true if the symbol is a definition of a function or data object.
    fn is_definition(&self) -> bool {
        let n_type = self.n_type();
        n_type & macho::N_STAB == 0 && n_type & macho::N_TYPE != macho::N_UNDF
    }
}

impl<Endian: endian::Endian> Nlist for macho::Nlist32<Endian> {
    type Word = u32;
    type Endian = Endian;

    fn n_strx(&self, endian: Self::Endian) -> u32 {
        self.n_strx.get(endian)
    }
    fn n_type(&self) -> u8 {
        self.n_type
    }
    fn n_sect(&self) -> u8 {
        self.n_sect
    }
    fn n_desc(&self, endian: Self::Endian) -> u16 {
        self.n_desc.get(endian)
    }
    fn n_value(&self, endian: Self::Endian) -> Self::Word {
        self.n_value.get(endian)
    }
}

impl<Endian: endian::Endian> Nlist for macho::Nlist64<Endian> {
    type Word = u64;
    type Endian = Endian;

    fn n_strx(&self, endian: Self::Endian) -> u32 {
        self.n_strx.get(endian)
    }
    fn n_type(&self) -> u8 {
        self.n_type
    }
    fn n_sect(&self) -> u8 {
        self.n_sect
    }
    fn n_desc(&self, endian: Self::Endian) -> u16 {
        self.n_desc.get(endian)
    }
    fn n_value(&self, endian: Self::Endian) -> Self::Word {
        self.n_value.get(endian)
    }
}
