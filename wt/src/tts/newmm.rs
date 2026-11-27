//! Thai word tokenizer: PyThaiNLP 5.4's newmm (dictionary maximal matching constrained by Thai
//! Character Cluster boundaries), ported as is so the words, and so the phonemes, come out the
//! same. Positions are character indices, as in Python.

use std::collections::HashMap;
use std::sync::LazyLock;

/// PyThaiNLP's Trie: words added stripped; prefixes() lists the words starting at a position,
/// shortest first.
pub struct Trie {
    nodes: Vec<Node>,
}

#[derive(Default)]
struct Node {
    end: bool,
    children: HashMap<char, usize>,
}

impl Trie {
    pub fn new<'a>(words: impl IntoIterator<Item = &'a str>) -> Trie {
        let mut t = Trie { nodes: vec![Node::default()] };
        for w in words {
            t.add(crate::util::py_strip(w));
        }
        t
    }

    fn add(&mut self, word: &str) {
        let mut cur = 0;
        for ch in word.chars() {
            cur = match self.nodes[cur].children.get(&ch) {
                Some(&n) => n,
                None => {
                    self.nodes.push(Node::default());
                    let n = self.nodes.len() - 1;
                    self.nodes[cur].children.insert(ch, n);
                    n
                }
            };
        }
        self.nodes[cur].end = true;
    }

    /// Lengths of the words in the trie that start at `start`, shortest first.
    pub fn prefixes(&self, text: &[char], start: usize) -> Vec<usize> {
        let mut res = vec![];
        let mut cur = 0;
        for (i, ch) in text.iter().enumerate().skip(start) {
            let Some(&n) = self.nodes[cur].children.get(ch) else { break };
            if self.nodes[n].end {
                res.push(i + 1 - start);
            }
            cur = n;
        }
        res
    }
}

/// Python's str.splitlines().
pub fn py_splitlines(text: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = 0;
    let mut it = text.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}') {
            out.push(&text[start..i]);
            let mut next = i + c.len_utf8();
            if c == '\r' && it.peek().is_some_and(|&(_, n)| n == '\n') {
                it.next();
                next += 1;
            }
            start = next;
        }
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

// ---- Thai Character Clusters (pythainlp.tokenize.tcc_p) -----------------------------------------

static TCC: LazyLock<fancy_regex::Regex> = LazyLock::new(|| {
    let rules = "เc็ck
เcctาะk
เccีtยะk
เccีtย(?=[เ-ไก-ฮ]|$)k
เcc็ck
เcิc์ck
เcิtck
เcีtยะ?k
เcืtอะ?k
เc[ิีุู]tย(?=[เ-ไก-ฮ]|$)k
เctา?ะ?k
cัtวะk
c[ัื]tc[ุิะ]?k
c[ิุู]์
c[ะ-ู]tk
cรรc์
c็
ct[ะาำ]?k
ck
แc็c
แcc์
แctะ
แcc็c
แccc์
โctะ
[เ-ไ]ct
ก็
อึ
หึ
"
    .replace('k', "(cc?[dิ]?[์])?")
    .replace('c', "[ก-ฮ]")
    .replace('t', "[่-๋]?")
    .replace('d', "ูุ");
    let alts: Vec<&str> = rules.split_whitespace().collect();
    fancy_regex::Regex::new(&format!("^(?:{})", alts.join("|"))).unwrap()
});

/// Valid TCC break positions: index i is true when a cluster ends at character i.
fn tcc_pos_array(text: &[char]) -> Vec<bool> {
    let mut arr = vec![false; text.len() + 1];
    let s: String = text.iter().collect();
    let offsets: Vec<usize> = s.char_indices().map(|(b, _)| b).chain([s.len()]).collect();
    let mut p = 0;
    while p < text.len() {
        let n = match TCC.find(&s[offsets[p]..]) {
            Ok(Some(m)) if m.end() > 0 => s[offsets[p]..offsets[p] + m.end()].chars().count(),
            _ => 1,
        };
        p += n;
        arr[p] = true;
    }
    arr
}

// ---- newmm (pythainlp.tokenize.newmm) -------------------------------------------------------------

static NONTHAI: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^(?:[-a-zA-Z]+|\d+([,\.]\d+)*|[ \t]+|\r?\n|[^\x{0E00}-\x{0E7F} \t\r\n]+)").unwrap()
});

const MAX_GRAPH_SIZE: usize = 50;

struct Text {
    chars: Vec<char>,
    s: String,
    offsets: Vec<usize>,
}

impl Text {
    fn new(chars: &[char]) -> Text {
        let s: String = chars.iter().collect();
        let offsets = s.char_indices().map(|(b, _)| b).chain([s.len()]).collect();
        Text { chars: chars.to_vec(), s, offsets }
    }

    /// _PAT_NONTHAI.match(text, pos): the end of the match, in characters.
    fn nonthai_end(&self, pos: usize) -> Option<usize> {
        let m = NONTHAI.find(&self.s[self.offsets[pos]..])?;
        Some(pos + self.s[self.offsets[pos]..self.offsets[pos] + m.end()].chars().count())
    }
}

fn two_consonants(word: &[char]) -> bool {
    word.len() <= 2 && word.iter().all(|c| ('ก'..='ฮ').contains(c))
}

fn bfs_first_path(graph: &HashMap<usize, Vec<usize>>, start: usize, goal: usize) -> Option<Vec<usize>> {
    let mut visited = std::collections::HashSet::from([start]);
    let mut queue = std::collections::VecDeque::from([(start, vec![start])]);
    while let Some((vertex, path)) = queue.pop_front() {
        for &pos in graph.get(&vertex).map(Vec::as_slice).unwrap_or(&[]) {
            if pos == goal {
                let mut p = path.clone();
                p.push(pos);
                return Some(p);
            } else if visited.insert(pos) {
                let mut p = path.clone();
                p.push(pos);
                queue.push_back((pos, p));
            }
        }
    }
    None
}

/// A min-heap of positions with Python heapq's behavior where it matters here: pop the smallest,
/// peek the smallest, membership.
#[derive(Default)]
struct PosList(Vec<usize>);

impl PosList {
    fn push(&mut self, p: usize) {
        self.0.push(p);
    }
    fn pop(&mut self) -> usize {
        let i = (0..self.0.len()).min_by_key(|&i| self.0[i]).unwrap();
        self.0.swap_remove(i)
    }
    fn min(&self) -> usize {
        *self.0.iter().min().unwrap()
    }
}

fn onecut(text: &[char], dict: &Trie) -> Vec<Vec<char>> {
    let t = Text::new(text);
    let mut out = vec![];
    let mut graph: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut graph_size = 0;
    let valid = tcc_pos_array(text);
    let len_text = text.len();
    let mut pos_list = PosList(vec![0]);
    let mut end_pos = 0;
    while pos_list.min() < len_text {
        let begin_pos = pos_list.pop();
        // _extend_graph
        for len in dict.prefixes(text, begin_pos) {
            let end = begin_pos + len;
            if valid[end] {
                graph.entry(begin_pos).or_default().push(end);
                graph_size += 1;
                if !pos_list.0.contains(&end) {
                    pos_list.push(end);
                }
                if graph_size > MAX_GRAPH_SIZE {
                    break;
                }
            }
        }
        match pos_list.0.len() {
            1 => {
                // one candidate, no longer ambiguous
                let path = bfs_first_path(&graph, end_pos, pos_list.min()).unwrap_or_else(|| vec![end_pos, pos_list.min()]);
                graph_size = 0;
                graph.clear();
                for &pos in &path[1..] {
                    out.push(text[end_pos..pos].to_vec());
                    end_pos = pos;
                }
            }
            0 => {
                // no candidate, deal with non-dictionary word
                end_pos = find_skip_end(&t, begin_pos, &valid, dict);
                graph_size = 0;
                graph.clear();
                out.push(text[begin_pos..end_pos].to_vec());
                pos_list.push(end_pos);
            }
            _ => {}
        }
    }
    let _ = t.chars.len();
    out
}

fn find_skip_end(t: &Text, begin_pos: usize, valid: &[bool], dict: &Trie) -> usize {
    let len_text = t.chars.len();
    if let Some(end) = t.nonthai_end(begin_pos) {
        return end; // non-Thai token, skip to the end
    }
    // Thai token, find minimum skip
    for pos in begin_pos + 1..len_text {
        if valid[pos] {
            let longer = dict.prefixes(&t.chars, pos).into_iter().any(|len| valid[pos + len] && !two_consonants(&t.chars[pos..pos + len]));
            if longer {
                return pos; // is a Thai token that longer than 2 chars
            }
            if t.nonthai_end(pos).is_some() {
                return pos; // is a non-Thai token
            }
        }
    }
    len_text
}

static DIGITS_WITH_SEPARATOR: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(\d+[\.\,:])+\d+").unwrap());

/// Rejoin formatted numbers (times, decimals, IP addresses) a tokenizer split into several words.
fn rejoin_formatted_num(segments: Vec<String>) -> Vec<String> {
    let original: String = segments.concat();
    let to_char = |b: usize| original[..b].chars().count();
    let mut matches = DIGITS_WITH_SEPARATOR.find_iter(&original).map(|m| (to_char(m.start()), to_char(m.end())));
    let mut joined = vec![];
    let (mut pos, mut idx) = (0usize, 0usize);
    let mut m = matches.next();
    while idx < segments.len() {
        let Some((start, end)) = m else { break };
        if pos >= start {
            let mut connected = String::new();
            while pos < end && idx < segments.len() {
                connected.push_str(&segments[idx]);
                pos += segments[idx].chars().count();
                idx += 1;
            }
            if !connected.is_empty() {
                joined.push(connected);
            }
            m = matches.next();
        } else {
            pos += segments[idx].chars().count();
            joined.push(segments[idx].clone());
            idx += 1;
        }
    }
    joined.extend(segments[idx..].iter().cloned());
    joined
}

/// pythainlp word_tokenize(text, custom_dict, engine="newmm", keep_whitespace=True,
/// join_broken_num=True).
pub fn word_tokenize(text: &str, dict: &Trie) -> Vec<String> {
    if text.is_empty() {
        return vec![];
    }
    let chars: Vec<char> = text.chars().collect();
    let words: Vec<String> = onecut(&chars, dict).into_iter().map(|w| w.into_iter().collect()).collect();
    rejoin_formatted_num(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_with_a_dictionary() {
        let dict = Trie::new(["โอเค", "บ่", "พวกเรา", "รัก", "ภาษา", "บ้านเกิด", "พวก", "เรา", "บ้าน", "เกิด"]);
        assert_eq!(word_tokenize("โอเคบ่พวกเรารักภาษาบ้านเกิด", &dict), ["โอเค", "บ่", "พวกเรา", "รัก", "ภาษา", "บ้านเกิด"]);
    }

    #[test]
    fn rejoins_numbers() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(rejoin_formatted_num(s(&["เวลา", " ", "12", ":", "00น", " ", "1", ",", "234", ".", "5"])), s(&["เวลา", " ", "12:00น", " ", "1,234.5"]));
    }
}
