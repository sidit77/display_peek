use windows::core::HSTRING;

fn find_terminal_idx(content: &[u16]) -> usize {
    for (i, val) in content.iter().enumerate() {
        if *val == 0 {
            return i;
        }
    }
    content.len()
}

pub fn convert_u16_to_string(data: &[u16]) -> String {
    let terminal_idx = find_terminal_idx(data);
    HSTRING::from_wide(&data[0..terminal_idx]).to_string_lossy()
}

pub struct U8Iter {
    value: u8,
    size: u32
}

impl U8Iter {
    pub fn new(value: u8) -> Self {
        Self {
            value,
            size: u8::BITS,
        }
    }
}

impl Iterator for U8Iter {
    type Item = bool;

    fn next(&mut self) -> Option<Self::Item> {
        if self.size > 0 {
            let result = self.value & 0x80 != 0x0;
            self.size -= 1;
            self.value <<= 1;
            Some(result)
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (u8::BITS as usize, Some(u8::BITS as usize))
    }
}

impl ExactSizeIterator for U8Iter {

}