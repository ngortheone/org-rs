//    This file is part of org-rs.
//
//    org-rs is free software: you can redistribute it and/or modify
//    it under the terms of the GNU General Public License as published by
//    the Free Software Foundation, either version 3 of the License, or
//    (at your option) any later version.
//
//    org-rs is distributed in the hope that it will be useful,
//    but WITHOUT ANY WARRANTY; without even the implied warranty of
//    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
//    GNU General Public License for more details.
//
//    You should have received a copy of the GNU General Public License
//    along with org-rs.  If not, see <https://www.gnu.org/licenses/>.

// Parts of the cursor code are shamelessly copied from xi-rope
// https://github.com/xi-editor/xi-editor/tree/master/rust/rope

use crate::data::Interval;
use memchr::{memchr, memrchr};
use regex::{Captures, Match, Regex};
use std::borrow::Cow;

use crate::headline::{REGEX_HEADLINE_MULTILINE, REGEX_HEADLINE_SHORT};

lazy_static! {
    pub static ref REGEX_EMPTY_LINE: Regex = Regex::new(r"^[ \t]*$").unwrap();
}

/// Metric is an addrress of special kind of marker.
/// Metric by itself does represent a user-facing value (e.g. char, string..)
pub trait Metric {
    /// Is this metric located by given offset in a given string
    fn is_boundary(s: &str, offset: usize) -> bool;

    /// Try to find previous metric relative the given offset in a given string
    fn prev(s: &str, offset: usize) -> Option<usize>;

    /// Try to find next metric relative the given offset in a given string
    fn next(s: &str, offset: usize) -> Option<usize>;

    fn at_or_next(s: &str, offset: usize) -> Option<usize> {
        if Self::is_boundary(s, offset) {
            Some(offset)
        } else {
            Self::next(s, offset)
        }
    }

    fn at_or_prev(s: &str, offset: usize) -> Option<usize> {
        if Self::is_boundary(s, offset) {
            Some(offset)
        } else {
            Self::prev(s, offset)
        }
    }
}

/// UTF Char metric. The addrress of UTF char is is the address of it's first byte
pub struct CharMetric;

impl CharMetric {
    /// Given the inital byte of a UTF-8 codepoint, returns the number of
    /// bytes required to represent the codepoint.
    /// RFC reference : https://tools.ietf.org/html/rfc3629#section-4
    /// TODO maybe rename to len()
    pub fn len_utf8_from_first_byte(b: u8) -> usize {
        match b {
            b if b < 0x80 => 1,
            b if b < 0xe0 => 2,
            b if b < 0xf0 => 3,
            _ => 4,
        }
    }
}

impl Metric for CharMetric {
    fn is_boundary(s: &str, offset: usize) -> bool {
        s.is_char_boundary(offset)
    }

    fn prev(s: &str, offset: usize) -> Option<usize> {
        if offset == 0 {
            None
        } else {
            let mut len = 1;
            while !s.is_char_boundary(offset - len) {
                len += 1;
            }
            Some(offset - len)
        }
    }

    fn next(s: &str, offset: usize) -> Option<usize> {
        if offset == s.len() {
            None
        } else {
            let b = s.as_bytes()[offset];
            Some(offset + CharMetric::len_utf8_from_first_byte(b))
        }
    }
}

/// Newline metric. Literally the addrress of '\n' byte
pub struct NewlineMetric;

impl Metric for NewlineMetric {
    fn is_boundary(s: &str, offset: usize) -> bool {
        if offset == 0 {
            false
        } else {
            s.as_bytes()[offset - 1] == b'\n'
        }
    }

    fn prev(s: &str, offset: usize) -> Option<usize> {
        debug_assert!(offset > 0, "caller is responsible for validating input");
        memrchr(b'\n', &s.as_bytes()[..offset - 1]).map(|pos| pos + 1)
    }

    fn next(s: &str, offset: usize) -> Option<usize> {
        memchr(b'\n', &s.as_bytes()[offset..]).map(|pos| offset + pos + 1)
    }
}

struct Addressable<T> {
    value: T,
    address: usize,
}

/// Lexeme is anything that represents a meaaningful value to a parser (e.g. char, string).
/// Usually lexeme is delimited by:
/// - 2 metrics, e.g. [CharMetric..CharMetric) == char
/// - beginning of input and a metric, e.g. [..NewlineMetric] == Line
/// - metric and end of input - char or line and the end of input
pub trait Lexeme {
    type Item;

    fn prev(s: &str, offset: usize) -> Option<Addressable<Self::Item>>;

    fn next(s: &str, offset: usize) -> Option<Addressable<Self::Item>>;
}

struct CharLexeme;

impl Lexeme for CharLexeme {
    type Item = char;

    fn prev(s: &str, offset: usize) -> Option<Addressable<Self::Item>> {
        if let Some(a) = CharMetric::prev(s, offset) {
            s[a..].chars().next().map(|c| Addressable {
                value: c,
                address: a,
            })
        } else {
            None
        }
    }

    fn next(s: &str, offset: usize) -> Option<Addressable<Self::Item>> {
        if let Some(a) = CharMetric::next(s, offset) {
            s[a..].chars().next().map(|c| Addressable {
                value: c,
                address: a,
            })
        } else {
            None
        }
    }
}

struct LineLexeme;

impl<'a> Lexeme for LineLexeme {
    type Item = Cow<'a, str>;

    fn prev(s: &str, offset: usize) -> Option<Addressable<Self::Item>> {
        unimplemented!()
    }

    fn next(s: &str, offset: usize) -> Option<Addressable<Self::Item>> {
        unimplemented!()
    }
}

pub trait Cursor {
    // total length of the underlying data
    fn data_len() -> usize;

    fn pos() -> usize;
    fn set(pos: usize) -> ();

    fn inc(inc: usize) -> usize;
    fn dec(dec: usize) -> usize;

    fn is_boundary<M: Metric>(&self) -> bool;
    fn goto_prev<M: Metric>(&mut self) -> Option<usize>;
    fn goto_next<M: Metric>(&mut self) -> Option<usize>;
    fn at_or_next<M: Metric>(&mut self) -> Option<usize>;
    fn at_or_prev<M: Metric>(&mut self) -> Option<usize>;

    fn prev<M: Lexeme>(&mut self) -> Option<M::Item>;
    fn next<M: Lexeme>(&mut self) -> Option<M::Item>;
}

pub struct StrCursor<'a> {
    data: &'a str,
    pos: usize,
}

impl<'a> StrCursor<'a> {
    pub fn new(data: &'a str, pos: usize) -> Cursor<'a> {
        Cursor { data, pos }
    }

    pub fn set(&mut self, pos: usize) {
        self.pos = pos;
    }

    pub fn inc(&mut self, inc: usize) {
        self.pos = self.pos + inc;
    }

    pub fn dec(&mut self, dec: usize) {
        if dec > self.pos {
            self.pos = 0;
        } else {
            self.pos = self.pos - dec;
        }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn data(&self) -> &str {
        self.data
    }

    pub fn next<M: Metric>(&mut self) -> Option<usize> {
        if let Some(l) = M::next(self.data, self.pos) {
            self.pos = l;
            Some(l)
        } else {
            None
        }
    }

    pub fn is_boundary<M: Metric>(&self) -> bool {
        M::is_boundary(self.data, self.pos)
    }

    pub fn prev<M: Metric>(&mut self) -> Option<usize> {
        if let Some(offset) = M::prev(self.data, self.pos) {
            self.pos = offset;
            Some(offset)
        } else {
            None
        }
    }

    /// Skip over space, tabs and newline characters
    /// Cursor position is set before next non-whitespace char
    pub fn skip_whitespace(&mut self) -> usize {
        while let Some(c) = self.get_next_char() {
            if !(c.is_whitespace()) {
                self.get_prev_char();
                break;
            } else {
                self.get_next_char();
            }
        }
        self.pos()
    }

    /// Moves cursor to the beginning of the current line.
    /// Acts like "Home" button
    /// If cursor is already at the beginning of the line - nothing happens
    /// Returns the position of the cursor
    pub fn goto_line_begin(&mut self) -> usize {
        if self.pos() != 0 && self.at_or_prev::<NewlineMetric>().is_none() {
            self.set(0);
        }
        self.pos()
    }

    /// Moves cursor to the beginning of the next line. If there is no next line
    /// cursor position is set to len() of the input
    pub fn goto_next_line(&mut self) -> usize {
        let res = self.next::<NewlineMetric>();
        match res {
            None => {
                self.set(self.data.len());
                self.data.len()
            }
            Some(x) => x,
        }
    }

    /// Moves cursor to the beginning of the previous line.
    /// If there is no previous line then cursor position
    /// is set the beginning of the rope - 0
    pub fn goto_prev_line(&mut self) -> usize {
        // move to the beginning of the current line
        self.goto_line_begin();
        if self.pos() == 0 {
            return 0;
        }
        let res = self.prev::<NewlineMetric>();

        match res {
            None => {
                self.set(0);
                0
            }
            Some(x) => x,
        }
    }

    /// Return the character position of the first character on the current line.
    /// If N is none then acts as `goto_line_begin`
    /// Otherwise moves forward N - 1 lines first.
    /// with N < 1 cursor will move to previous lines
    ///
    /// Corresponds to `line-beginning-position` in elisp
    /// This function does not move the cursor (does save-excursion)
    pub fn line_beginning_position(&mut self, n: Option<i32>) -> usize {
        let pos = self.pos();
        match n {
            None | Some(1) => {
                self.goto_line_begin();
            }

            Some(x) => {
                if x > 1 {
                    for _p in 0..x - 1 {
                        self.goto_next_line();
                    }
                } else {
                    self.goto_line_begin();
                    if self.pos() != 0 {
                        for p in 0..(x - 1).abs() {
                            if self.prev::<NewlineMetric>().is_none() {
                                self.set(0);
                                break;
                            }
                        }
                    }
                }
            }
        }

        let result = self.pos();
        self.set(pos);
        return result;
    }

    /// Return the character position of the last character on the current line.
    /// With argument N not nil or 1, move forward N - 1 lines first.
    /// If scan reaches end of buffer, return that position.
    ///
    /// Corresponds to `line-end-position` in elisp
    /// This function does not move the cursor (does save-excursion)
    pub fn line_end_position(&mut self, n: Option<i32>) -> usize {
        let pos = self.pos();
        match n {
            None | Some(1) => {
                self.goto_next_line();
            }

            Some(x) => {
                if x > 1 {
                    for _p in 0..x {
                        self.goto_next_line();
                    }
                } else if self.pos() != 0 {
                    for p in 0..=x.abs() {
                        if self.prev::<NewlineMetric>().is_none() {
                            break;
                        }
                    }
                }
            }
        }

        let result = self.prev::<CharMetric>().unwrap_or(0);
        self.set(pos);
        return result;
    }

    // TODO refactor to use BaseMetric
    pub fn char_after(&mut self, offset: usize) -> Option<char> {
        let pos = self.pos();
        self.set(offset);
        let result = self.get_next_char();
        self.set(pos);
        return result;
    }

    /// Checks if current line matches a given regex
    /// This function determines whether the text in
    /// the current buffer directly following cursor matches
    /// the regular expression regexp.
    /// “Directly following” means precisely that:
    /// the search is “anchored” and it can succeed only
    /// starting with the first character following point.
    /// The result is true if so, false otherwise.
    /// This function does not move cursor
    /// Use `capturing_at` if you need capture groups.
    pub fn looking_at(&self, re: &Regex) -> Option<Match<'a>> {
        let end = if !is_multiline_regex(re.as_str()) {
            NewlineMetric::next(self.data, self.pos)
                .map(|p| p - 1) // exclude '\n' from the string'
                .unwrap_or_else(|| self.data.len())
        } else {
            self.data.len()
        };
        re.find(&self.data[self.pos..end])
    }

    /// Acts exactly as `looking_at` but returns Captures
    /// This is slower than simple regex search so if you don't need
    /// capture groups use `looking_at` for better performance
    pub fn capturing_at(&self, re: &Regex) -> Option<Captures<'a>> {
        let end = if !is_multiline_regex(re.as_str()) {
            NewlineMetric::next(self.data, self.pos)
                .map(|p| p - 1) // exclude '\n' from the string'
                .unwrap_or_else(|| self.data.len())
        } else {
            self.data.len()
        };

        re.captures(&self.data[self.pos..end])
    }

    pub fn is_bol(&self) -> bool {
        if self.pos == 0 {
            true
        } else {
            NewlineMetric::is_boundary(self.data, self.pos)
        }
    }

    /// Search forward from point to str. Sets point to the end of the
    /// occurence found and returns point. bound is a position in the
    /// buffer. The match found must not end after that position. If
    /// None then search to end of the buffer. If count is specified,
    /// find the countth occurence. If countth occurence is not found
    /// None is returned. If count is not provided then 1 is used as
    /// count. Note that searching backward is not supported like it
    /// is in the elisp equivalent.
    pub fn search_forward(
        &mut self,
        str: &str,
        bound: Option<usize>,
        count: Option<usize>,
    ) -> Option<usize> {
        let count = match count {
            Some(count) => count,
            _ => 1,
        };

        let bound = match bound {
            Some(bound) => bound,
            _ => self.data.len(),
        };

        let pos = self.pos();
        if bound < pos {
            return None;
        }

        let mut iter = self.data[pos..].match_indices(str);
        let mut i = 1;
        loop {
            match iter.next() {
                Some(result) => {
                    if result.0 + pos + str.len() > bound {
                        return None;
                    }

                    if count == i {
                        self.set(result.0 + pos + str.len());
                        return Some(result.0 + pos + str.len());
                    }

                    i += 1;
                }
                None => return None,
            }
        }
    }

    ///
    /// Search forward from point for regular expression REGEXP.
    /// Set point to the end of the occurrence found, and return match Interval
    /// with absolute positions.
    /// Original implementation returned cursor position and modified global variables
    /// with match data
    ///
    /// The optional second argument BOUND is a buffer position that bounds
    ///   the search.  The match found must not end after that position.  A
    ///   value of nil means search to the end of the accessible portion of
    ///   the buffer.
    /// elisp:`(re-search-forward REGEXP &optional BOUND NOERROR COUNT)`
    pub fn re_search_forward(&mut self, re: &Regex, bound: Option<usize>) -> Option<Interval> {
        let end = bound.unwrap_or(self.data.len());

        if end <= self.pos {
            return None;
        }

        /// Set point to the end of the occurrence found, and return point.
        match re.find(&self.data[self.pos..end]) {
            None => None,
            Some(m) => {
                let res = Interval::new(self.pos + m.start(), self.pos + m.end());
                self.set(self.pos + m.end());
                Some(res)
            }
        }
    }

    /// Moves point forward, stopping before a char not in str, or at position limit.
    pub fn skip_chars_forward(&mut self, str: &str, limit: Option<usize>) -> usize {
        let pos = self.pos();
        let limit = match limit {
            Some(lim) => lim,
            _ => self.data.len(),
        };

        if pos >= limit {
            return 0;
        }

        let mut count = 0;
        while let Some(c) = self.get_next_char() {
            if !str.contains(c) {
                self.get_prev_char();
                return count;
            }
            if count + pos > limit {
                self.get_prev_char();
                return count;
            }
            count += 1;
        }
        count
    }

    /// Move point backward, stopping after a char not in str, or at `limit`
    /// `limit` - is an absolute buffer position
    /// Returns the distance traveled.
    ///
    /// Difference with Emacs variant is that emacs returs negative number
    ///
    /// (skip-chars-backward STRING &optional LIM)
    pub fn skip_chars_backward(&mut self, str: &str, limit: Option<usize>) -> usize {
        let limit = match limit {
            Some(lim) => lim,
            _ => 0,
        };

        if self.pos <= limit {
            return 0;
        }

        let mut count = 0;
        while let Some(c) = self.get_prev_char() {
            if !str.contains(c) {
                self.get_next_char();
                return count;
            }
            if self.pos < limit {
                self.get_next_char();
                return count;
            }
            count += 1;
        }
        count
    }
}

/// Checks if a regular expression can match multiple lines.
pub fn is_multiline_regex(regex: &str) -> bool {
    // regex characters that match line breaks
    // todo: currently multiline mode is ignored
    let multiline_indicators = vec![r"\n", r"\r", r"[[:space:]]"];

    multiline_indicators.iter().any(|&i| regex.contains(i))
}

mod test {

    use super::Cursor;
    use super::Metric;
    use super::NewlineMetric;
    use super::REGEX_EMPTY_LINE;

    use crate::data::Syntax;
    use crate::headline::REGEX_HEADLINE_SHORT;
    use crate::parser::Parser;

    use crate::cursor::CharMetric;
    use regex::Match;
    use regex::Regex;

    #[test]
    fn essentials() {
        let input = "1234567890\nЗдравствуйте";
        let mut cursor = Cursor::new(&input, 0);
        assert_eq!('1', cursor.get_next_char().unwrap());
        assert_eq!(1, cursor.pos());
        assert_eq!('2', cursor.get_next_char().unwrap());
        assert_eq!(2, cursor.pos());
        assert_eq!(11, cursor.next::<NewlineMetric>().unwrap());
        assert!(cursor.is_boundary::<NewlineMetric>());
        assert_eq!('З', cursor.get_next_char().unwrap());
        assert_eq!(13, cursor.pos());
        cursor.set(12);
        assert!(!cursor.is_boundary::<CharMetric>());
    }

    #[test]
    fn looking_at_headline() {
        let rope = "Some text\n**** headline\n";
        let mut cursor = Cursor::new(&rope, 0);
        assert!(cursor.looking_at(&*REGEX_HEADLINE_SHORT).is_none());

        cursor.set(4);
        assert!(cursor.looking_at(&*REGEX_HEADLINE_SHORT).is_none());
        assert_eq!(4, cursor.pos());

        cursor.set(15);
        assert!(cursor.looking_at(&*REGEX_HEADLINE_SHORT).is_none());

        cursor.set(10);

        let m = cursor.looking_at(&*REGEX_HEADLINE_SHORT).unwrap();
        assert_eq!(0, m.start());
        assert_eq!(5, m.end());
        assert_eq!("**** ", m.as_str());
        assert_eq!(10, cursor.pos());
    }

    #[test]
    fn looking_at_empty_line_re() {
        let text = "First line\n   \n\nFourth line";
        let mut cursor = Cursor::new(&text, 0);

        assert!(cursor.looking_at(&*REGEX_EMPTY_LINE).is_none());
        cursor.goto_next_line();
        assert!(cursor.looking_at(&*REGEX_EMPTY_LINE).is_some());
        cursor.goto_next_line();
        assert!(cursor.looking_at(&*REGEX_EMPTY_LINE).is_some());
        cursor.goto_next_line();
        assert!(cursor.looking_at(&*REGEX_EMPTY_LINE).is_none());
    }

    #[test]
    fn skip_whitespaces() {
        let rope = " \n\t\rorg-mode ";
        let mut cursor = Cursor::new(&rope, 0);
        cursor.skip_whitespace();
        assert_eq!(cursor.get_next_char().unwrap(), 'o');

        let rope2 = "no_whitespace_for_you!";
        cursor = Cursor::new(&rope2, 0);
        cursor.skip_whitespace();
        assert_eq!(cursor.get_next_char().unwrap(), 'n');

        // Skipping all the remaining whitespace results in invalid cursor at the end of the rope
        let rope3 = " ";
        cursor = Cursor::new(&rope3, 0);
        cursor.skip_whitespace();
        assert_eq!(None, cursor.get_next_char());
    }

    #[test]
    fn line_begin() {
        let rope = "First line\nSecond line\r\nThird line";
        let mut cursor = Cursor::new(&rope, 13);
        assert_eq!(cursor.goto_line_begin(), 11);
        assert_eq!(cursor.goto_line_begin(), 11);
        assert_eq!(cursor.goto_line_begin(), 11);
        cursor.set(26);
        assert_eq!(cursor.goto_line_begin(), 24);
        assert!(cursor.is_bol());
        assert_eq!(cursor.get_next_char().unwrap(), 'T');
        assert_eq!(cursor.goto_line_begin(), 24);
        assert_eq!(cursor.get_next_char().unwrap(), 'T');
        cursor.set(3);
        assert_eq!(cursor.goto_line_begin(), 0);
        assert_eq!(cursor.get_next_char().unwrap(), 'F');
    }

    #[test]
    fn prev_line() {
        let rope = "First line\nSecond line\r\nThird line\nFour";
        let mut cursor = Cursor::new(&rope, rope.len());

        assert_eq!(cursor.goto_prev_line(), 24);
        assert_eq!(cursor.get_next_char().unwrap(), 'T');

        assert_eq!(cursor.goto_prev_line(), 11);
        assert_eq!(cursor.get_next_char().unwrap(), 'S');

        assert_eq!(cursor.goto_prev_line(), 0);
        assert_eq!(cursor.get_next_char().unwrap(), 'F');
    }

    #[test]
    fn line_begin_pos() {
        let rope = "One\nTwo\nThi\nFo4\nFiv\nSix\n7en";
        let mut cursor = Cursor::new(&rope, 13);

        assert_eq!(cursor.line_beginning_position(None), 12);
        assert_eq!(cursor.line_beginning_position(Some(1)), 12);
        assert_eq!(cursor.line_beginning_position(Some(2)), 16);
        assert_eq!(cursor.line_beginning_position(Some(3)), 20);

        assert_eq!(cursor.line_beginning_position(Some(0)), 8);
        assert_eq!(cursor.line_beginning_position(Some(-1)), 4);
        assert_eq!(cursor.line_beginning_position(Some(-2)), 0);
    }

    #[test]
    fn line_end_pos() {
        let text = "One\nTwo\nThi\nFo4\nFiv\nSix\n7en";
        let mut cursor = Cursor::new(&text, 13);

        assert_eq!(27, text.len());
        // Moving forward
        assert_eq!(cursor.line_end_position(None), 15);
        assert_eq!(cursor.line_end_position(Some(1)), 15);
        assert_eq!(cursor.line_end_position(Some(2)), 19);
        assert_eq!(cursor.line_end_position(Some(3)), 23);
        assert_eq!(cursor.line_end_position(Some(4)), 26);

        //Moving backward
        assert_eq!(cursor.line_end_position(Some(0)), 11);
        assert_eq!(cursor.line_end_position(Some(-1)), 7);
        assert_eq!(cursor.line_end_position(Some(-2)), 3);
        assert_eq!(cursor.line_end_position(Some(-3)), 3);
    }

    #[test]
    fn is_bol() {
        let rope = "One\nTwo\nThi\nFo4\nFiv\nSix\n7en";
        let mut cursor = Cursor::new(&rope, 0);
        assert!(cursor.is_bol());
        cursor.set(2);
        assert!(!cursor.is_bol());
        cursor.set(4);
        assert!(cursor.is_bol());
        cursor.set(rope.len());
        assert!(!cursor.is_bol());

        cursor.prev::<NewlineMetric>();
        assert!(cursor.is_bol());
        cursor.goto_prev_line();
        assert!(cursor.is_bol());
        cursor.goto_next_line();
        assert!(cursor.is_bol());
    }

    #[test]
    fn search_forward() {
        let str = "onetwothreefouronetwothreeonetwothreeonetwothreefouroneabababa";
        let mut cursor = Cursor::new(&str, 0);
        assert_eq!(cursor.search_forward("one", None, Some(2)), Some(18));
        assert_eq!(cursor.search_forward("one", None, None), Some(29));
        cursor.set(0);
        assert_eq!(cursor.search_forward("threeone", Some(10), None), None); // there is no match before 10th pos
        assert_eq!(cursor.search_forward("threeone", Some(100), Some(10)), None); // there is not a 10th match so return None
        assert_eq!(cursor.search_forward("two", None, Some(4)), Some(43));
        assert_eq!(cursor.pos(), 43);
        assert_eq!(cursor.search_forward("aba", Some(10), None), None); // bound is before current pos
        assert_eq!(cursor.pos(), 43);
        assert_eq!(cursor.search_forward("aba", Some(10000), Some(2)), Some(62));
        cursor.set(0);
        assert_eq!(cursor.search_forward("aba", Some(10000), Some(6)), None);
    }

    #[test]
    fn skip_chars_forward() {
        let str = "  k\t **hello";
        let mut cursor = Cursor::new(&str, 0);
        assert_eq!(cursor.skip_chars_forward(" ", None), 2);
        assert_eq!(cursor.pos(), 2);
        assert_eq!(cursor.skip_chars_forward(" k\t", None), 3);
        cursor.set(0);
        assert_eq!(cursor.skip_chars_forward("* k\t", Some(2)), 3);
    }

    #[test]
    fn skip_chars_backward() {
        let text = "This is some text 123 \t\n\r";
        let mut cursor = Cursor::new(&text, text.len());
        assert_eq!(8, cursor.skip_chars_backward(" \t\n\r123", None));
        assert_eq!(17, cursor.pos());
        assert_eq!(' ', cursor.get_next_char().unwrap());

        cursor.set(text.len());
        assert_eq!(1, cursor.skip_chars_backward(" \t\n\r", Some(24)));
        assert_eq!('\r', cursor.get_next_char().unwrap());

        let txt2 = "Text";
        cursor = Cursor::new(&txt2, txt2.len());
        assert_eq!(0, cursor.skip_chars_backward("", None));
    }

    #[test]
    fn re_search_forward() {
        let text = "One\nTwo\nThi\nFo4\nFiv\nSix\n7en";
        let mut cursor = Cursor::new(&text, 0);

        let re = Regex::new(r"\d").unwrap();
        assert_eq!(14, cursor.re_search_forward(&re, None).unwrap().start);
        assert_eq!(15, cursor.pos());
        assert_eq!(None, cursor.re_search_forward(&re, Some(10)));
        assert_eq!(15, cursor.pos());
        assert_eq!(24, cursor.re_search_forward(&re, Some(25)).unwrap().start);
        assert_eq!(25, cursor.pos());
        assert_eq!(None, cursor.re_search_forward(&re, Some(24)));
        assert_eq!(25, cursor.pos());
    }
}
