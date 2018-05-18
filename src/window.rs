use std::io::{Read, Result};
use std::mem;
use std::cmp::min;
use ::Window;


impl<R:Read> Window<R> {
    pub fn new(mut reader:R, block_size: usize) -> Result<Window<R>> {
        let mut front = vec!(0;block_size);
        let mut back = vec!(0;block_size);
        let size = try!(reader.read(front.as_mut_slice()));
        unsafe {
            front.set_len(size);
        }
        let size = try!(reader.read(back.as_mut_slice()));
        unsafe {
            back.set_len(size);
        }
        Ok(Window {
            front,
            back,
            block_size,
            offset: 0,
            reader,
            bytes_read: 0
        })
    }

    pub fn advance(&mut self) -> Result<(Option<u8>, Option<u8>)> {
        if self.front.len() == 0 {
            return Ok((None, None));
        }

        if self.offset >= self.front.len() {
            if self.back.len() == 0 {
                return Ok((None, None));
            }
            try!(self.load_next_block());
        }
        let tail = self.front[self.offset];
        let head = self.get_head();
        self.offset += 1;
        self.bytes_read += 1;
        Ok((Some(tail), head))
    }

    fn get_head(&self) -> Option<u8> {
        let head_index = self.offset + self.block_size - self.front.len();
        if head_index >= self.back.len() {
            return None;
        }
        return Some(self.back[head_index]);
    }

    fn load_next_block(&mut self) -> Result<()> {
        // We've gone past the end of the front half
        self.front = mem::replace(&mut self.back, vec!(0;self.block_size));
        let size = try!(self.reader.read(self.back.as_mut_slice()));
        unsafe{
            self.back.set_len(size);
        }
        self.offset = 0;
        Ok(())
    }

    pub fn frame<'a>(&'a self) -> (&'a [u8], &'a [u8]) {
        let front_offset = min(self.offset, self.front.len());
        let back_offset = min(self.offset, self.back.len());
        (&self.front[front_offset..], &self.back[..back_offset])
    }

    pub fn frame_size(&self) -> usize {
        self.front.len() + self.back.len() - self.offset
    }

    pub fn on_boundry(&self) -> bool {
        self.offset == 0 || self.offset == self.front.len()
    }

    pub fn get_bytes_read(&self) -> usize {
        self.bytes_read
    }
}

#[cfg(test)]
mod test {
    use super::super::Window;
    use std::io::Cursor;
    #[test]
    fn frame_iterator() {
        let mut window_basic = Window::new(Cursor::new(vec![1, 2, 3, 4, 5, 6 ,7, 8, 9, 10]), 5).unwrap();
        //assert_eq!(window_basic.frame().map(|a| *a).collect::<Vec<u8>>(), vec![1, 2, 3, 4, 5]);
        assert_eq!(window_basic.frame(), (&[1, 2, 3, 4, 5][..], &[][..]));

        window_basic.advance().unwrap();
        // assert_eq!(window_basic.frame().map(|a| *a).collect::<Vec<u8>>(), vec![2, 3, 4, 5, 6]);
        assert_eq!(window_basic.frame(), (&[2, 3, 4, 5][..], &[6][..]));

        window_basic.advance().unwrap();
        window_basic.advance().unwrap();
        window_basic.advance().unwrap();
        window_basic.advance().unwrap();
        assert_eq!(window_basic.frame(), (&[][..], &[6, 7, 8, 9, 10][..]));


        window_basic.advance().unwrap();
        assert_eq!(window_basic.frame(), (&[7, 8, 9, 10][..], &[][..]));

        window_basic.advance().unwrap();
        window_basic.advance().unwrap();
        window_basic.advance().unwrap();
        assert_eq!(window_basic.frame(), (&[10][..], &[][..]));

         window_basic.advance().unwrap();
        assert_eq!(window_basic.frame(), (&[][..], &[][..]));


        let window_too_small = Window::new(Cursor::new(vec![1, 2, 3, 4]), 5).unwrap();
        assert_eq!(window_too_small.frame(), (&[1, 2, 3, 4][..], &[][..]));

        let window_empty = Window::new(Cursor::new(vec![]), 5).unwrap();
        assert_eq!(window_empty.frame(), (&[][..], &[][..]));

        let mut window_bigger = Window::new(Cursor::new(vec![1, 2, 3, 4, 5, 6 ,7, 8, 9, 10, 11, 12]), 5).unwrap();
        assert_eq!(window_bigger.frame(), (&[1, 2, 3, 4, 5][..], &[][..]));
        window_bigger.advance().unwrap();
        window_bigger.advance().unwrap();
        window_bigger.advance().unwrap();
        window_bigger.advance().unwrap();
        window_bigger.advance().unwrap();
        window_bigger.advance().unwrap();
        assert_eq!(window_bigger.frame(), (&[7, 8, 9, 10][..], &[11][..]));

        window_bigger.advance().unwrap();
        assert_eq!(window_bigger.frame(), (&[8, 9, 10][..], &[11, 12][..]));
        window_bigger.advance().unwrap();
        assert_eq!(window_bigger.frame(), (&[9, 10][..], &[11, 12][..]));
        window_bigger.advance().unwrap();
        assert_eq!(window_bigger.frame(), (&[10][..], &[11, 12][..]));
        window_bigger.advance().unwrap();
        assert_eq!(window_bigger.frame(), (&[][..], &[11, 12][..]));
        window_bigger.advance().unwrap();
        assert_eq!(window_bigger.frame(), (&[12][..], &[][..]));

    }
    #[test]
    fn advance() {
        let mut window_basic = Window::new(Cursor::new(vec![1, 2, 3, 4, 5, 6 ,7, 8, 9, 10]), 5).unwrap();
        assert_eq!(window_basic.advance().unwrap(), (Some(1), Some(6)));
        assert_eq!(window_basic.advance().unwrap(), (Some(2), Some(7)));
        assert_eq!(window_basic.advance().unwrap(), (Some(3), Some(8)));
        assert_eq!(window_basic.advance().unwrap(), (Some(4), Some(9)));
        assert_eq!(window_basic.advance().unwrap(), (Some(5), Some(10)));
        assert_eq!(window_basic.advance().unwrap(), (Some(6), None));
        assert_eq!(window_basic.advance().unwrap(), (Some(7), None));
        assert_eq!(window_basic.advance().unwrap(), (Some(8), None));
        assert_eq!(window_basic.advance().unwrap(), (Some(9), None));
        assert_eq!(window_basic.advance().unwrap(), (Some(10), None));
        assert_eq!(window_basic.advance().unwrap(), (None, None));

        let mut window_huge = Window::new(Cursor::new(vec![1, 2, 3, 4, 5, 6 ,7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18]), 5).unwrap();
        assert_eq!(window_huge.advance().unwrap(), (Some(1), Some(6)));
        assert_eq!(window_huge.advance().unwrap(), (Some(2), Some(7)));
        assert_eq!(window_huge.advance().unwrap(), (Some(3), Some(8)));
        assert_eq!(window_huge.advance().unwrap(), (Some(4), Some(9)));
        assert_eq!(window_huge.advance().unwrap(), (Some(5), Some(10)));
        assert_eq!(window_huge.advance().unwrap(), (Some(6), Some(11)));
        assert_eq!(window_huge.advance().unwrap(), (Some(7), Some(12)));
        assert_eq!(window_huge.advance().unwrap(), (Some(8), Some(13)));
        assert_eq!(window_huge.advance().unwrap(), (Some(9), Some(14)));
        assert_eq!(window_huge.advance().unwrap(), (Some(10), Some(15)));
        assert_eq!(window_huge.advance().unwrap(), (Some(11), Some(16)));
        assert_eq!(window_huge.advance().unwrap(), (Some(12), Some(17)));
        assert_eq!(window_huge.advance().unwrap(), (Some(13), Some(18)));
        assert_eq!(window_huge.advance().unwrap(), (Some(14), None));
        assert_eq!(window_huge.advance().unwrap(), (Some(15), None));
        assert_eq!(window_huge.advance().unwrap(), (Some(16), None));
        assert_eq!(window_huge.advance().unwrap(), (Some(17), None));
        assert_eq!(window_huge.advance().unwrap(), (Some(18), None));
        assert_eq!(window_huge.advance().unwrap(), (None, None));

        let mut window_empty = Window::new(Cursor::new(vec![]), 5).unwrap();
        assert_eq!(window_empty.advance().unwrap(), (None, None));

        let mut window_too_small = Window::new(Cursor::new(vec![1, 2, 3, 4]), 5).unwrap();
        assert_eq!(window_too_small.advance().unwrap(), (Some(1), None));
        assert_eq!(window_too_small.advance().unwrap(), (Some(2), None));
        assert_eq!(window_too_small.advance().unwrap(), (Some(3), None));
        assert_eq!(window_too_small.advance().unwrap(), (Some(4), None));
        assert_eq!(window_too_small.advance().unwrap(), (None, None));
        assert_eq!(window_too_small.advance().unwrap(), (None, None));
        assert_eq!(window_too_small.advance().unwrap(), (None, None));
    }
}
