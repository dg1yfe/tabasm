//! The memory image and the object regions cut from it.
//!
//! The image is a flat 64 KB array with a written/unwritten flag per byte; a
//! region is a maximal run of written bytes, and regions are what the object
//! writers turn into records.

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

    /// Write one byte. An address past the end of the image is **wrapped to 16
    /// bits** and the byte is still written; returns false so the caller can
    /// diagnose the first such address, using `reported_out_of_range` to keep
    /// it to one per run.
    ///
    /// 2.3: the region keeps the *unwrapped* address, so a run that crosses
    /// `0xFFFF` stays one record rather than splitting -- `.org $FFFC` then five
    /// bytes is a single five-byte record at FFFC. Only the image index wraps.
    pub fn write(&mut self, addr: u32, byte: u8) -> bool {
        let in_range = (addr as usize) < IMAGE_SIZE;
        let idx = (addr as usize) % IMAGE_SIZE;
        self.mem[idx] = byte;
        // The watermarks describe the image, so they take the wrapped address.
        let w = idx as u32;
        self.lo = Some(self.lo.map_or(w, |l| l.min(w)));
        self.hi = Some(self.hi.map_or(w, |h| h.max(w)));

        match &mut self.cur {
            Some(r) if r.start + r.bytes.len() as u32 == addr => r.bytes.push(byte),
            _ => {
                self.flush();
                self.cur = Some(Region {
                    start: addr,
                    bytes: vec![byte],
                });
            }
        }
        in_range
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
