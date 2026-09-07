//! The symbol table (3.7, the specification).

use crate::limits::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Segment {
    Null,
    Code,
    Bit,
    Extd,
    Data,
}

impl Segment {
    /// The letter shown in the label table and the symbol file (9.8, 9.6).
    pub fn letter(self) -> char {
        match self {
            Segment::Null => 'N',
            Segment::Code => 'C',
            Segment::Bit => 'B',
            Segment::Extd => 'X',
            Segment::Data => 'D',
        }
    }
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    pub value: i32,
    pub segment: Segment,
    pub local: bool,
    pub exported: bool,
}

pub struct Symbols {
    /// Definition order is preserved: 9.8 says the -l label table lists
    /// symbols in definition order, not sorted.
    pub list: Vec<Symbol>,
    index: std::collections::HashMap<String, usize>,
    pub ignore_case: bool,
    pub overflowed: bool,
}

impl Symbols {
    pub fn new(ignore_case: bool) -> Symbols {
        Symbols {
            list: Vec::new(),
            index: std::collections::HashMap::new(),
            ignore_case,
            overflowed: false,
        }
    }

    fn key(&self, name: &str) -> String {
        if self.ignore_case {
            name.to_ascii_uppercase()
        } else {
            name.to_string()
        }
    }

    /// 3.7/10.2: 31 usable characters; a longer name is truncated and
    /// diagnosed, and assembly continues with the truncated name -- so two
    /// names sharing a 31-character prefix genuinely collide.
    pub fn truncate(name: &str) -> (String, bool) {
        if name.len() > MAX_LABEL {
            (name.chars().take(MAX_LABEL).collect(), true)
        } else {
            (name.to_string(), false)
        }
    }

    /// 3.7: a symbol beginning with the local-label character is stored as
    /// `<module>.<symbol>`, the module defaulting to `noname`. Because the
    /// default local character is '_', every symbol starting with an
    /// underscore is already a local label, intended or not.
    pub fn qualify(name: &str, local_char: u8, module: &str) -> (String, bool) {
        if name.as_bytes().first() == Some(&local_char) {
            (format!("{}.{}", module, name), true)
        } else {
            (name.to_string(), false)
        }
    }

    pub fn get(&self, name: &str) -> Option<&Symbol> {
        self.index.get(&self.key(name)).map(|i| &self.list[*i])
    }

    pub fn value(&self, name: &str) -> Option<i32> {
        self.get(name).map(|s| s.value)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.index.contains_key(&self.key(name))
    }

    /// Define a symbol. Returns false if the name already exists, which the
    /// caller reports as `Duplicate label:` -- 4.5 says the second definition
    /// is discarded and the first wins.
    pub fn define(&mut self, name: &str, value: i32, segment: Segment, local: bool) -> bool {
        let k = self.key(name);
        if let Some(i) = self.index.get(&k) {
            let _ = i;
            return false;
        }
        // 10.1: the symbol table overflowing is NOT fatal. The label is
        // dropped, `label table overflow` is logged, and the run continues.
        if self.list.len() >= MAX_SYMBOLS {
            self.overflowed = true;
            return true;
        }
        self.index.insert(k, self.list.len());
        self.list.push(Symbol {
            name: name.to_string(),
            value,
            segment,
            local,
            exported: false,
        });
        true
    }

    /// Re-assign an existing symbol. 4.5: `.SET` cannot create one, and unlike
    /// `.EQU` it may be applied repeatedly without a duplicate-label error.
    pub fn set(&mut self, name: &str, value: i32) -> bool {
        let k = self.key(name);
        match self.index.get(&k) {
            Some(&i) => {
                self.list[i].value = value;
                true
            }
            None => false,
        }
    }

    /// Used by pass 2 to overwrite pass-1 values without a duplicate error.
    pub fn redefine(&mut self, name: &str, value: i32) {
        let k = self.key(name);
        if let Some(&i) = self.index.get(&k) {
            self.list[i].value = value;
        }
    }

    pub fn export(&mut self, name: &str) {
        let k = self.key(name);
        if let Some(&i) = self.index.get(&k) {
            self.list[i].exported = true;
        }
    }
}
