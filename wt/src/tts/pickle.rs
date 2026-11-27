//! Reads the two pickled tables of the Thai fallback G2P (protocol 3): a dict of str -> int, and a
//! defaultdict(int) of (str, str, str) -> int. Only the opcodes those files use.

use anyhow::{Result, bail};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum Obj {
    Str(Rc<str>),
    Int(i64),
    Tuple(Rc<Vec<Obj>>),
    Dict(Rc<RefCell<Vec<(Obj, Obj)>>>),
    Global,
    Mark,
}

pub fn load(data: &[u8]) -> Result<Vec<(Obj, Obj)>> {
    let mut stack: Vec<Obj> = vec![];
    let mut memo: HashMap<u32, Obj> = HashMap::new();
    let mut i = 0usize;
    let take = |i: &mut usize, n: usize| -> Result<&[u8]> {
        if *i + n > data.len() {
            bail!("truncated pickle");
        }
        let s = &data[*i..*i + n];
        *i += n;
        Ok(s)
    };
    let u32le = |b: &[u8]| u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    loop {
        let op = take(&mut i, 1)?[0];
        match op {
            0x80 => {
                take(&mut i, 1)?; // PROTO
            }
            b'c' => {
                // GLOBAL module\nname\n
                for _ in 0..2 {
                    while take(&mut i, 1)?[0] != b'\n' {}
                }
                stack.push(Obj::Global);
            }
            b'q' => {
                let k = take(&mut i, 1)?[0] as u32;
                memo.insert(k, stack.last().cloned().unwrap_or(Obj::Mark));
            }
            b'r' => {
                let k = u32le(take(&mut i, 4)?);
                memo.insert(k, stack.last().cloned().unwrap_or(Obj::Mark));
            }
            b'h' => {
                let k = take(&mut i, 1)?[0] as u32;
                stack.push(memo.get(&k).cloned().unwrap_or(Obj::Mark));
            }
            b'j' => {
                let k = u32le(take(&mut i, 4)?);
                stack.push(memo.get(&k).cloned().unwrap_or(Obj::Mark));
            }
            b'(' => stack.push(Obj::Mark),
            b'X' => {
                let n = u32le(take(&mut i, 4)?) as usize;
                stack.push(Obj::Str(Rc::from(std::str::from_utf8(take(&mut i, n)?)?)));
            }
            b'K' => {
                let v = take(&mut i, 1)?[0] as i64;
                stack.push(Obj::Int(v));
            }
            b'M' => {
                let b = take(&mut i, 2)?;
                stack.push(Obj::Int(u16::from_le_bytes([b[0], b[1]]) as i64));
            }
            b'J' => {
                let b = take(&mut i, 4)?;
                stack.push(Obj::Int(i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64));
            }
            0x85 | 0x86 | 0x87 => {
                let n = (op - 0x84) as usize;
                if stack.len() < n {
                    bail!("bad pickle tuple");
                }
                let items = stack.split_off(stack.len() - n);
                stack.push(Obj::Tuple(Rc::new(items)));
            }
            b'}' => stack.push(Obj::Dict(Rc::new(RefCell::new(vec![])))),
            b'R' => {
                // defaultdict(int): an empty dict
                stack.pop();
                stack.pop();
                stack.push(Obj::Dict(Rc::new(RefCell::new(vec![]))));
            }
            b'u' => {
                let mark = stack.iter().rposition(|o| matches!(o, Obj::Mark)).ok_or_else(|| anyhow::anyhow!("SETITEMS without MARK"))?;
                let items = stack.split_off(mark + 1);
                stack.pop();
                let Some(Obj::Dict(d)) = stack.last() else { bail!("SETITEMS on a non-dict") };
                let mut d = d.borrow_mut();
                for kv in items.chunks(2) {
                    if let [k, v] = kv {
                        d.push((k.clone(), v.clone()));
                    }
                }
            }
            b'.' => {
                let Some(Obj::Dict(d)) = stack.pop() else { bail!("pickle is not a dict") };
                return Ok(Rc::try_unwrap(d).map(RefCell::into_inner).unwrap_or_else(|d| d.borrow().clone()));
            }
            other => bail!("unsupported pickle opcode 0x{other:02x} at {}", i - 1),
        }
    }
}

pub fn as_str(o: &Obj) -> Option<&str> {
    match o {
        Obj::Str(s) => Some(s),
        _ => None,
    }
}

pub fn as_int(o: &Obj) -> Option<i64> {
    match o {
        Obj::Int(v) => Some(*v),
        _ => None,
    }
}

pub fn as_tuple(o: &Obj) -> Option<&[Obj]> {
    match o {
        Obj::Tuple(t) => Some(t),
        _ => None,
    }
}
