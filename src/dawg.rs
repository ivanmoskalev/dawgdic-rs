use crate::pool::Pool;
use crate::unit::BaseType;

pub struct Dawg {
    base_pool: Pool<BaseUnit>,
    label_pool: Pool<u8>,
    flag_pool: Pool<bool>, // TODO: BitPool tool
    num_states: BaseType,
    num_merged_transitions: BaseType,
    num_merged_states: BaseType,
    num_merging_states: BaseType,
}

impl Dawg {
    pub fn child(&self, index: BaseType) -> BaseType {
        self.base_pool[index].child()
    }

    pub fn sibling(&self, index: BaseType) -> BaseType {
        if self.base_pool[index].has_sibling() {
            index + 1
        } else {
            0
        }
    }

    pub fn value(&self, index: BaseType) -> BaseType {
        self.base_pool[index].value()
    }

    pub fn is_leaf(&self, index: BaseType) -> bool {
        self.label(index) == 0
    }

    pub fn label(&self, index: BaseType) -> u8 {
        self.label_pool[index]
    }

    pub fn is_merging(&self, index: BaseType) -> bool {
        self.flag_pool[index]
    }

    pub fn states_count(&self) -> BaseType {
        self.num_states
    }

    pub fn merged_states_count(&self) -> BaseType {
        self.num_merged_states
    }

    pub fn transition_count(&self) -> BaseType {
        self.base_pool.len() - 1
    }

    pub fn merged_transitions_count(&self) -> BaseType {
        self.num_merged_transitions
    }

    pub fn merging_states_count(&self) -> BaseType {
        self.num_merging_states
    }

    pub fn print(&self) {
        for item in self.base_pool.iter() {
            println!("{}", item.base())
        }
    }
}

#[repr(transparent)]
#[derive(Copy, Clone)]
struct BaseUnit(BaseType);

impl BaseUnit {
    fn child(&self) -> BaseType {
        self.0 >> 2
    }

    fn has_sibling(&self) -> bool {
        self.0 & 1 != 0
    }

    fn value(&self) -> BaseType {
        self.0 >> 1
    }

    fn is_state(&self) -> bool {
        self.0 & 2 != 0
    }

    fn set_base(&mut self, base: BaseType) {
        self.0 = base
    }

    fn base(&self) -> BaseType {
        self.0
    }
}

// BUILDER

#[derive(Default)]
pub struct DawgBuilder {
    base_pool: Pool<BaseUnit>,
    label_pool: Pool<u8>,
    flag_pool: Pool<bool>,
    unit_pool: Pool<DawgUnit>,
    hash_table: Pool<BaseType>,
    unfixed_units: Vec<BaseType>,
    unused_units: Vec<BaseType>,
    num_states: BaseType,
    num_merged_transitions: BaseType,
    num_merging_states: BaseType,
}

impl DawgBuilder {
    pub fn new() -> DawgBuilder {
        let mut builder: DawgBuilder = Default::default();
        builder.hash_table = {
            let mut hash_table = Vec::new();
            let initial_size = 1 << 8;
            hash_table.resize(initial_size, 0);
            Pool::from_vec(hash_table)
        };
        builder.num_states = 1;
        builder.reuse_or_create_unit();
        builder.allocate_transition();
        builder.unit_pool[0].set_label(0xFF);
        builder.unfixed_units.push(0);
        builder
    }

    pub fn insert_key(&mut self, key: &str, value: BaseType) -> Result<(), ()> {
        let mut bytes: Vec<u8> = key.bytes().collect();
        bytes.push(0);
        self.insert_key_bytes(&bytes, value)
    }

    fn insert_key_bytes(&mut self, key: &[u8], value: BaseType) -> Result<(), ()> {
        let mut index: BaseType = 0;
        let mut key_pos: usize = 0;

        // Find existing chain of units
        for (pos, byte) in key[0..].iter().enumerate() {
            key_pos = pos;
            let child_index = self.unit_pool[index].child;
            if child_index == 0 {
                break;
            }

            let key_label = byte;
            let unit_label = self.unit_pool[child_index].label;

            match key_label.cmp(&unit_label) {
                std::cmp::Ordering::Less => return Err(()),
                std::cmp::Ordering::Greater => {
                    self.unit_pool[child_index].set_has_sibling(true);
                    self.fix_units(child_index);
                    break;
                }
                std::cmp::Ordering::Equal => (),
            }

            index = child_index;
        }

        for byte in key[key_pos..].iter() {
            let child_index = self.reuse_or_create_unit();

            if self.unit_pool[index].child == 0 {
                self.unit_pool[child_index].set_is_state(true);
            }
            let child = self.unit_pool[index].child;
            self.unit_pool[child_index].set_sibling(child);
            self.unit_pool[child_index].set_label(*byte);
            self.unit_pool[index].set_child(child_index);
            self.unfixed_units.push(child_index);

            index = child_index;
        }

        self.unit_pool[index].set_value(value);

        Ok(())
    }

    pub fn build(mut self) -> Dawg {
        self.fix_units(0);
        self.base_pool[0].set_base(self.unit_pool[0].base());
        self.label_pool[0] = self.unit_pool[0].label;

        let num_transitions = self.base_pool.len() - 1;
        let num_merged_states = num_transitions + self.num_merged_transitions + 1 - self.num_states;
        Dawg {
            num_states: self.num_states,
            num_merged_transitions: self.num_merged_transitions,
            num_merged_states,
            num_merging_states: self.num_merging_states,
            base_pool: self.base_pool,
            label_pool: self.label_pool,
            flag_pool: self.flag_pool,
        }
    }

    fn fix_units(&mut self, index: BaseType) {
        while let Some(unfixed_index) = self.unfixed_units.pop() {
            if unfixed_index == index {
                break;
            }

            let hash_table_expansion_treshold =
                self.hash_table.len() - (self.hash_table.len() >> 2);
            if self.num_states >= hash_table_expansion_treshold {
                self.expand_hash_table();
            }

            let num_of_siblings: BaseType = {
                let mut count = 0;
                let mut i = unfixed_index;
                loop {
                    if i == 0 {
                        break;
                    }
                    count += 1;
                    i = self.unit_pool[i].sibling;
                }
                count
            };

            let unfixed_unit = self.find_unit(unfixed_index);
            let hash_id = unfixed_unit.hash_id;
            let mut matched_index: BaseType = unfixed_unit.transition_id;

            if matched_index != 0 {
                // TODO: avoid mutating lots of disparate fields
                self.num_merged_transitions += num_of_siblings;

                if !self.flag_pool[matched_index] {
                    self.num_merging_states += 1;
                    self.flag_pool[matched_index] = true;
                }
            } else {
                let mut transition_index = 0;
                for _ in 0..num_of_siblings {
                    transition_index = self.allocate_transition();
                }
                let mut i = unfixed_index;
                loop {
                    if i == 0 {
                        break;
                    }
                    self.base_pool[transition_index].set_base(self.unit_pool[i].base());
                    self.label_pool[transition_index] = self.unit_pool[i].label;
                    transition_index -= 1;
                    i = self.unit_pool[i].sibling;
                }
                matched_index = transition_index + 1;
                self.hash_table[hash_id] = matched_index;
                self.num_states += 1;
            }

            // Marking all fixed units for reuse
            let mut current = unfixed_index;
            let mut next = 0;
            loop {
                if current == 0 {
                    break;
                }
                next = self.unit_pool[current].sibling;
                self.mark_unit_as_unused(current);
                current = next;
            }

            let next_unfixed = self.unfixed_units.last().unwrap();
            self.unit_pool[*next_unfixed].set_child(matched_index);
        }
    }

    fn hash_transition(&self, index: BaseType) -> BaseType {
        let mut hash_value = 0;
        let mut index = index;
        loop {
            if index == 0 {
                break;
            }
            let base = self.base_pool[index].base();
            let label = self.label_pool[index];
            hash_value ^= Self::hash_from_base(base, label);
            if !self.base_pool[index].has_sibling() {
                break;
            }
            index += 1
        }
        hash_value
    }

    fn hash_unit(&self, index: BaseType) -> BaseType {
        let mut hash_value = 0;
        let mut index = index;
        loop {
            if index == 0 {
                break;
            }
            let base = self.unit_pool[index].base();
            let label = self.unit_pool[index].label;
            hash_value ^= Self::hash_from_base(base, label);
            index = self.unit_pool[index].sibling;
        }
        hash_value
    }

    fn find_unit(&self, unit_index: BaseType) -> FindUnitResult {
        let hash_table_size = self.hash_table.len();
        let mut hash_id = self.hash_unit(unit_index) % hash_table_size;
        loop {
            let transition_id = self.hash_table[hash_id];
            if transition_id == 0 {
                break;
            }
            if self.are_equal(unit_index, transition_id) {
                return FindUnitResult {
                    hash_id,
                    transition_id,
                };
            }
            hash_id = (hash_id + 1) % hash_table_size;
        }

        FindUnitResult {
            hash_id,
            transition_id: 0,
        }
    }

    fn are_equal(&self, unit_index: BaseType, transition_index: BaseType) -> bool {
        let mut i = self.unit_pool[unit_index].sibling;
        let mut transition_index = transition_index;
        loop {
            if i == 0 {
                break;
            }
            if !self.base_pool[transition_index].has_sibling() {
                return false;
            }
            transition_index += 1;
            i = self.unit_pool[i].sibling;
        }

        if self.base_pool[transition_index].has_sibling() {
            return false;
        }

        let mut i = unit_index;
        loop {
            if i == 0 {
                break;
            }
            if self.unit_pool[i].base() != self.base_pool[transition_index].base() {
                return false;
            }
            if self.unit_pool[i].label != self.label_pool[transition_index] {
                return false;
            }
            transition_index -= 1;
            i = self.unit_pool[i].sibling;
        }

        true
    }

    fn reuse_or_create_unit(&mut self) -> BaseType {
        let index = self.unused_units.pop().unwrap_or_else(|| {
            self.unit_pool.push(Default::default());
            self.unit_pool.len() - 1
        });
        self.unit_pool[index] = Default::default();
        index
    }

    #[inline]
    fn mark_unit_as_unused(&mut self, index: BaseType) {
        self.unused_units.push(index);
    }

    fn expand_hash_table(&mut self) {
        let hash_table_size = self.hash_table.len() << 1;
        self.hash_table.clear();
        self.hash_table.resize(hash_table_size, 0);

        // build new hash table
        for index in 1..self.base_pool.len() {
            if self.label_pool[index] == 0 || self.base_pool[index].is_state() {
                let transition = self.find_transition(index);
                self.hash_table[transition.hash_id] = index;
            }
        }
    }

    fn find_transition(&self, index: BaseType) -> FindUnitResult {
        let mut hash_id = self.hash_transition(index) % self.hash_table.len();
        loop {
            let transition_id = self.hash_table[hash_id];
            if transition_id == 0 {
                break;
            }
            hash_id = (hash_id + 1) % self.hash_table.len();
            // There must not be the same base value
        }
        FindUnitResult {
            hash_id,
            transition_id: 0,
        }
    }

    fn allocate_transition(&mut self) -> BaseType {
        self.flag_pool.push(false);
        self.base_pool.push(BaseUnit(0));
        let size = self.label_pool.len();
        self.label_pool.push(0);
        size
    }

    fn hash_from_base(value: BaseType, label: u8) -> BaseType {
        let value = value ^ (BaseType::from(label) << 24);
        let value = !value.overflowing_add(value << 15).0;
        let value = value ^ (value >> 12);
        let value = value.overflowing_add(value << 2).0;
        let value = value ^ (value >> 4);
        let value = value.overflowing_mul(2057).0;

        value ^ (value >> 16)
    }
}

#[derive(Default, Copy, Clone)]
struct DawgUnit {
    child: BaseType,
    sibling: BaseType,
    label: u8,
    is_state: bool,
    has_sibling: bool,
}

impl DawgUnit {
    fn base(&self) -> BaseType {
        if self.label == 0 {
            return (self.child << 1) | (if self.has_sibling { 1 } else { 0 });
        }
        (self.child << 2)
            | (if self.is_state { 2 } else { 0 })
            | (if self.has_sibling { 1 } else { 0 })
    }

    fn set_value(&mut self, value: BaseType) {
        self.child = value
    }

    fn set_child(&mut self, child: BaseType) {
        self.child = child;
    }

    fn set_sibling(&mut self, sibling: BaseType) {
        self.sibling = sibling;
    }

    fn set_label(&mut self, label: u8) {
        self.label = label
    }

    fn set_is_state(&mut self, is_state: bool) {
        self.is_state = is_state
    }

    fn set_has_sibling(&mut self, has_sibling: bool) {
        self.has_sibling = has_sibling
    }
}

#[derive(Copy, Clone)]
struct FindUnitResult {
    transition_id: BaseType,
    hash_id: BaseType,
}
