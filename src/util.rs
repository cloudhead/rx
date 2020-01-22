use rgx::math::Point2;
use rgx::rect::Rect;

pub fn clamp(p: &mut Point2<i32>, rect: Rect<i32>) {
    if p.x < rect.x1 {
        p.x = rect.x1;
    }
    if p.y < rect.y1 {
        p.y = rect.y1;
    }
    if p.x > rect.x2 {
        p.x = rect.x2;
    }
    if p.y > rect.y2 {
        p.y = rect.y2;
    }
}

#[macro_export]
macro_rules! hashmap {
    ($( $key: expr => $val: expr ),*) => {{
         let mut map = ::std::collections::HashMap::new();
         $( map.insert($key.to_string(), $val); )*
         map
    }}
}

// Copyright 2016 Bruce Mitchener, Jr. <bruce.mitchener@gmail.com>
// Portions copyright (C) 2012 Ingo Albrecht <prom@berlin.ccc.de>
// Licensed udner the MIT license.

/// Longest Common Prefix
///
/// Given a vector of string slices, calculate the string
/// slice that is the longest common prefix of the strings.
///
/// ```
/// use rx::util::longest_common_prefix;
///
/// let words = vec!["zebrawood", "zebrafish", "zebra mussel"];
/// assert_eq!(longest_common_prefix(words), "zebra");
///
/// assert_eq!(longest_common_prefix(vec![]), "");
/// assert_eq!(longest_common_prefix(vec!["ab"]), "ab");
/// assert_eq!(longest_common_prefix(vec!["a", "b", "c"]), "");
/// assert_eq!(longest_common_prefix(vec!["aba", "abb", "abc"]), "ab");
/// assert_eq!(longest_common_prefix(vec!["aba", "ab", "abc"]), "ab");
/// ```
pub fn longest_common_prefix(strings: Vec<&str>) -> &str {
    if let Some(first) = strings.first() {
        let first_bytes = first.as_bytes();
        let mut len = first.len();

        for s in &strings[1..] {
            len = std::cmp::min(
                len,
                s.as_bytes()
                    .iter()
                    .zip(first_bytes)
                    .take_while(|&(a, b)| a == b)
                    .count(),
            );
        }
        &first[..len]
    } else {
        ""
    }
}
