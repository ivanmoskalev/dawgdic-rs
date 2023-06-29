use crate::dawg::Dawg;
use crate::pool::Pool;
use crate::unit::BaseType;
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::convert::TryFrom;
use std::io::{Read, Write};

// Dictionary

pub struct Dictionary {
    root: u32,
    units: Pool<DictionaryUnit>,
}

impl Dictionary {
    pub fn from_reader<T: Read>(reader: &mut T) -> Option<Self> {
        let size = reader.read_u32::<LittleEndian>().ok()?;
        let size = usize::try_from(size).ok()?;
        let mut units = Vec::with_capacity(size);
        for _ in 0..size {
            let unit = reader.read_u32::<LittleEndian>().ok()?;
            units.push(DictionaryUnit(unit))
        }
        let units = Pool::from_vec(units);
        Some(Dictionary { root: 0, units })
    }

    pub fn write<T: Write>(&self, writer: &mut T) -> Result<(), ()> {
        let size = self.units.len();
        writer.write_u32::<LittleEndian>(size).expect("FIXME");
        for unit in self.units.iter() {
            writer.write_u32::<LittleEndian>(unit.0).expect("FIXME");
        }
        Ok(())
    }

    pub fn size(&self) -> BaseType {
        self.units.len()
    }

    pub fn has_value(&self, index: u32) -> bool {
        self.units
            .get(index)
            .map(|unit| unit.has_leaf())
            .unwrap_or(false)
    }

    pub fn value(&self, index: u32) -> Option<u32> {
        self.units
            .get(index)
            .map(|unit| index ^ unit.offset())
            .and_then(|value_unit_index| self.units.get(value_unit_index))
            .map(|unit| unit.value())
    }

    pub fn contains(&self, key: &[u8]) -> bool {
        self.follow_bytes(key, self.root)
            .map_or(false, |index| self.has_value(index))
    }

    pub fn find(&self, key: &[u8]) -> Option<u32> {
        self.follow_bytes(key, self.root)
            .and_then(|index| self.value(index))
    }

    pub fn follow(&self, label: u8, index: u32) -> Option<u32> {
        let unit = self.units[index];
        let next_index = index ^ unit.offset() ^ u32::from(label);
        let leaf_label = self.units[next_index].label();
        if leaf_label != u32::from(label) {
            return None;
        }
        Some(next_index)
    }

    pub fn follow_bytes(&self, key: &[u8], index: u32) -> Option<u32> {
        let mut index = index;
        for &ch in key {
            index = self.follow(ch, index)?;
        }
        Some(index)
    }
}

// Unit type

#[repr(transparent)]
#[derive(Copy, Clone)]
pub struct DictionaryUnit(pub u32);

const PRECISION_MASK: u32 = 0xFFFF_FFFF;
const OFFSET_MAX: u32 = 1 << 21;
const IS_LEAF_BIT: u32 = 1 << 31;
const HAS_LEAF_BIT: u32 = 1 << 8;
const EXTENSION_BIT: u32 = 1 << 9;

impl DictionaryUnit {
    pub fn has_leaf(&self) -> bool {
        self.0 & HAS_LEAF_BIT != 0
    }

    pub fn set_has_leaf(&mut self) {
        self.0 |= HAS_LEAF_BIT
    }

    pub fn value(&self) -> u32 {
        self.0 & (IS_LEAF_BIT ^ PRECISION_MASK)
    }

    pub fn set_value(&mut self, value: BaseType) {
        self.0 = value | IS_LEAF_BIT
    }

    pub fn label(&self) -> u32 {
        self.0 & (IS_LEAF_BIT | 0xFF)
    }

    pub fn set_label(&mut self, value: BaseType) {
        self.0 = (self.0 & !0xFF) | value
    }

    pub fn offset(&self) -> u32 {
        (self.0 >> 10) << ((self.0 & EXTENSION_BIT) >> 6)
    }

    pub fn set_offset(&mut self, offset: BaseType) -> bool {
        if offset >= (OFFSET_MAX << 8) {
            return false;
        }
        self.0 &= IS_LEAF_BIT | HAS_LEAF_BIT | 0xFF;
        if offset < OFFSET_MAX {
            self.0 |= offset << 10;
        } else {
            self.0 |= (offset << 2) | EXTENSION_BIT;
        }
        true
    }
}

pub struct DictionaryBuilder {
    dawg: Dawg,
    units: Pool<DictionaryUnit>,
    extras: Pool<DictionaryExtra>,
    labels: Pool<u8>,
    link_table: std::collections::HashMap<BaseType, BaseType>,
    unfixed_index: BaseType,
    num_unused_nuts: BaseType,
}

const UPPER_MASK: BaseType = !(OFFSET_MAX - 1);
const LOWER_MASK: BaseType = 0xFF;

impl DictionaryBuilder {
    pub fn new(dawg: Dawg) -> DictionaryBuilder {
        DictionaryBuilder {
            dawg,
            units: Default::default(),
            extras: Default::default(),
            labels: Default::default(),
            link_table: Default::default(),
            unfixed_index: 0,
            num_unused_nuts: 0,
        }
    }

    pub fn build(mut self) -> Dictionary {
        self.reserve_unit(0);
        self.extra(0).set_is_used();
        self.units[0].set_offset(1);
        self.units[0].set_label(0);

        self.build_dictionary_indexes(0, 0);

        self.fix_all_blocks();

        Dictionary {
            root: 0,
            units: self.units,
        }
    }

    fn build_dictionary_indexes(&mut self, dawg_index: BaseType, dic_index: BaseType) -> bool {
        if self.dawg.is_leaf(dawg_index) {
            return true;
        }

        let dawg_child_index = self.dawg.child(dawg_index);
        if self.dawg.is_merging(dawg_child_index) {
            let offset = self.link_table.get(&dawg_child_index);
            if let Some(offset) = offset {
                let offset = offset ^ dic_index;
                if (offset & UPPER_MASK == 0) || (offset & LOWER_MASK == 0) {
                    if self.dawg.is_leaf(dawg_child_index) {
                        self.units[dic_index].set_has_leaf();
                    }
                    self.units[dic_index].set_offset(offset);
                    return true;
                }
            }
        }

        let offset = self.arrange_child_nodes(dawg_index, dic_index);
        if offset == 0 {
            return false;
        }

        if self.dawg.is_merging(dawg_child_index) {
            self.link_table.insert(dawg_child_index, offset);
        }

        let mut dawg_child_index = dawg_child_index;
        loop {
            let dic_child_index = offset ^ BaseType::from(self.dawg.label(dawg_child_index));
            if !self.build_dictionary_indexes(dawg_child_index, dic_child_index) {
                return false;
            }
            dawg_child_index = self.dawg.sibling(dawg_child_index);
            if dawg_child_index == 0 {
                break;
            }
        }

        true
    }

    fn reserve_unit(&mut self, index: BaseType) {
        if index >= self.units.len() {
            self.expand_dictionary();
        }

        if index == self.unfixed_index {
            self.unfixed_index = self.extra(index).next();
            if self.unfixed_index == index {
                self.unfixed_index = self.units.len();
            }
        }

        {
            let prev = self.extra(index).prev();
            let next = self.extra(index).next();
            self.extra(prev).set_next(next);
        }
        {
            let prev = self.extra(index).prev();
            let next = self.extra(index).next();
            self.extra(next).set_prev(prev);
        }
        self.extra(index).set_is_fixed();
    }

    fn fix_all_blocks(&mut self) {
        const NUM_OF_UNFIXED_BLOCKS: BaseType = 16;
        let begin = {
            if self.extras.len() > NUM_OF_UNFIXED_BLOCKS {
                self.extras.len() - NUM_OF_UNFIXED_BLOCKS
            } else {
                0
            }
        };
        let end = self.extras.len();

        for block_id in begin..end {
            self.fix_block(block_id);
        }
    }

    fn fix_block(&mut self, block_id: BaseType) {
        const BLOCK_SIZE: BaseType = 1;
        let begin = block_id * BLOCK_SIZE;
        let end = begin + BLOCK_SIZE;

        let mut unused_offset_for_label = 0;
        for offset in begin..end {
            if !self.extra(offset).is_used() {
                unused_offset_for_label = offset;
                break;
            }
        }

        for index in begin..end {
            if !self.extra(index).is_fixed() {
                self.reserve_unit(index);
                self.units[index].set_label(index ^ unused_offset_for_label);
                self.num_unused_nuts += 1;
            }
        }
    }

    fn arrange_child_nodes(&mut self, dawg_index: BaseType, dic_index: BaseType) -> BaseType {
        self.labels.clear();

        let mut dawg_child_index = self.dawg.child(dawg_index);
        while dawg_child_index != 0 {
            self.labels.push(self.dawg.label(dawg_child_index));
            dawg_child_index = self.dawg.sibling(dawg_child_index);
        }

        let offset = self.find_good_offset(dic_index);
        if !self.units[dic_index].set_offset(dic_index ^ offset) {
            return 0;
        }

        dawg_child_index = self.dawg.child(dawg_index);

        for i in 0..self.labels.len() {
            let dic_child_index = offset ^ BaseType::from(self.labels[i]);
            self.reserve_unit(dic_child_index);

            if self.dawg.is_leaf(dawg_child_index) {
                self.units[dic_index].set_has_leaf();
                self.units[dic_child_index].set_value(self.dawg.value(dawg_child_index))
            } else {
                self.units[dic_child_index].set_label(BaseType::from(self.labels[i]));
            }
            dawg_child_index = self.dawg.sibling(dawg_child_index);
        }

        self.extra(offset).set_is_used();

        offset
    }

    fn find_good_offset(&self, index: BaseType) -> BaseType {
        if self.unfixed_index >= self.units.len() {
            return self.units.len() | (index & 0xFF);
        }

        let mut unfixed_index = self.unfixed_index;
        loop {
            let offset = unfixed_index ^ BaseType::from(self.labels[0]);
            if self.is_good_offset(index, offset) {
                return offset;
            }
            unfixed_index = self.get_extra(unfixed_index).next();
            if unfixed_index == self.unfixed_index {
                break;
            }
        }

        self.units.len() | (index & 0xFF)
    }

    fn is_good_offset(&self, index: BaseType, offset: BaseType) -> bool {
        if self.get_extra(offset).is_used() {
            return false;
        }

        let relative_offset = index ^ offset;
        if (relative_offset & LOWER_MASK != 0) && (relative_offset & UPPER_MASK != 0) {
            return false;
        }

        for i in 1..self.labels.len() {
            let extra_index = offset ^ BaseType::from(self.labels[i]);
            if self.get_extra(extra_index).is_fixed() {
                return false;
            }
        }

        true
    }

    fn expand_dictionary(&mut self) {
        let src_num_units = self.units.len();
        let src_num_blocks = self.extras.len();

        let dest_num_units = src_num_units + 256;
        let dest_num_blocks = src_num_blocks + 256;

        if dest_num_blocks > 16 * 256 {
            self.fix_block(src_num_blocks - 16 * 256);
        }

        self.units.resize(dest_num_units, DictionaryUnit(0));
        self.extras
            .resize(dest_num_blocks, DictionaryExtra { hi: 0, lo: 0 });

        for i in (src_num_units + 1)..dest_num_units {
            self.extra(i - 1).set_next(i);
            self.extra(i).set_prev(i - 1);
        }

        self.extra(src_num_units).set_prev(dest_num_units - 1);
        self.extra(dest_num_units - 1).set_next(src_num_units);

        let unfixed_index = self.unfixed_index;
        let prev = self.extra(unfixed_index).prev();
        self.extra(src_num_units).set_prev(prev);
        self.extra(dest_num_units - 1).set_next(unfixed_index);

        let prev = self.extra(self.unfixed_index).prev();
        self.extra(prev).set_next(src_num_units);
        self.extra(self.unfixed_index).set_prev(dest_num_units - 1);
    }

    fn extra(&mut self, i: BaseType) -> &mut DictionaryExtra {
        &mut self.extras[i]
    }

    fn get_extra(&self, i: BaseType) -> DictionaryExtra {
        self.extras[i]
    }
}

#[derive(Copy, Clone)]
struct DictionaryExtra {
    lo: BaseType,
    hi: BaseType,
}

impl DictionaryExtra {
    fn set_is_fixed(&mut self) {
        self.lo |= 1;
    }

    fn set_next(&mut self, next: BaseType) {
        self.lo = (self.lo & 1) | (next << 1);
    }

    fn set_is_used(&mut self) {
        self.hi |= 1;
    }

    fn set_prev(&mut self, prev: BaseType) {
        self.hi = (self.hi & 1) | (prev << 1);
    }

    fn is_fixed(&self) -> bool {
        self.lo & 1 == 1
    }

    fn next(&self) -> BaseType {
        self.lo >> 1
    }

    fn is_used(&self) -> bool {
        self.hi & 1 == 1
    }

    fn prev(&self) -> BaseType {
        self.hi >> 1
    }
}
