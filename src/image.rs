//! The memory image and the object regions cut from it (2.3 & 2.5).

use crate::limits::IMAGE_SIZE;

/// A run of consecutive bytes that actually generated code. 8.1: records are
/// emitted for regions that contain code, not for the whole address space, and
/// a region ends when the counter jumps -- at an .ORG, or across a .BLOCK,
/// which reserves space without emitting.
#[derive(Clone, Debug)]
pub struct Region {
    pub start: u32,
    pub bytes: Vec<u8>,
}

pub struct Image {
    mem: Vec<u8>,
    /// Watermarks over what was actually written, used by block output (-c).
    pub lo: Option<u32>,
    pub hi: Option<u32>,
    pub regions: Vec<Region>,
    cur: Option<Region>,
    /// 2.3: one diagnostic for the FIRST out-of-range address in the run, not
    /// one per byte.
    pub reported_out_of_range: bool,
}

impl Image {
    pub fn new(fill: u8) -> Image {
        Image {
            mem: vec![fill; IMAGE_SIZE],
            lo: None,
            hi: None,
            regions: Vec::new(),
            cur: None,
            reported_out_of_range: false,
        }
    }

    /// Write one byte. Returns false if the address lies outside the image, in
    /// which case the byte is DROPPED -- 2.3 is explicit that it is neither
    /// wrapped nor truncated. The caller decides whether to diagnose, using
    /// `reported_out_of_range` to keep it to one per run.
    pub fn write(&mut self, addr: u32, byte: u8) -> bool {
        if addr as usize >= IMAGE_SIZE {
            return false;
        }
        self.mem[addr as usize] = byte;
        self.lo = Some(self.lo.map_or(addr, |l| l.min(addr)));
        self.hi = Some(self.hi.map_or(addr, |h| h.max(addr)));

        match &mut self.cur {
            Some(r) if r.start + r.bytes.len() as u32 == addr => r.bytes.push(byte),
            _ => {
                self.flush();
                self.cur = Some(Region { start: addr, bytes: vec![byte] });
            }
        }
        true
    }

    /// 2.3: a read outside the image returns 0.
    pub fn read(&self, addr: u32) -> u8 {
        if (addr as usize) < IMAGE_SIZE {
            self.mem[addr as usize]
        } else {
            0
        }
    }

    pub fn flush(&mut self) {
        if let Some(r) = self.cur.take() {
            if !r.bytes.is_empty() {
                self.regions.push(r);
            }
        }
    }

    /// 8.1: with -c or -b the whole span from the lowest to the highest
    /// address used is written as one run, gaps carrying whatever the image
    /// holds -- 0x00, or the -f fill byte.
    pub fn block(&self) -> Option<Region> {
        let (lo, hi) = (self.lo?, self.hi?);
        Some(Region {
            start: lo,
            bytes: self.mem[lo as usize..=hi as usize].to_vec(),
        })
    }
}
