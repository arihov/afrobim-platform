// SPDX-License-Identifier: LicenseRef-AfroBIM-Proprietary
// Copyright (c) 2026 AfroBIM. All rights reserved.
//! A small XML writer: nesting, escaping, attributes. Text is escaped and characters XML 1.0
//! does not allow are dropped.

pub(crate) fn valid_xml_char(c: char) -> bool {
    matches!(c, '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')
}

pub(crate) fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars().filter(|c| valid_xml_char(*c)) {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            '\r' => o.push_str("&#13;"),
            '\t' => o.push_str("&#9;"),
            c => o.push(c),
        }
    }
    o
}

pub(crate) struct XmlWriter {
    out: String,
    stack: Vec<&'static str>,
}

impl XmlWriter {
    pub fn new() -> XmlWriter {
        XmlWriter { out: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"), stack: Vec::new() }
    }

    fn indent(&mut self) {
        for _ in 0..self.stack.len() {
            self.out.push_str("  ");
        }
    }

    fn attrs(&mut self, attrs: &[(&str, &str)]) {
        for (k, v) in attrs {
            self.out.push_str(&format!(" {k}=\"{}\"", escape(v)));
        }
    }

    pub fn open(&mut self, name: &'static str, attrs: &[(&str, &str)]) {
        self.indent();
        self.out.push('<');
        self.out.push_str(name);
        self.attrs(attrs);
        self.out.push_str(">\n");
        self.stack.push(name);
    }

    pub fn close(&mut self) {
        let name = self.stack.pop().expect("unbalanced close");
        self.indent();
        self.out.push_str(&format!("</{name}>\n"));
    }

    /// `<name attrs/>`.
    pub fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) {
        self.indent();
        self.out.push('<');
        self.out.push_str(name);
        self.attrs(attrs);
        self.out.push_str("/>\n");
    }

    /// `<name>text</name>` on one line.
    pub fn text(&mut self, name: &str, text: &str) {
        self.text_attrs(name, &[], text);
    }

    pub fn text_attrs(&mut self, name: &str, attrs: &[(&str, &str)], text: &str) {
        self.indent();
        self.out.push('<');
        self.out.push_str(name);
        self.attrs(attrs);
        self.out.push('>');
        self.out.push_str(&escape(text));
        self.out.push_str(&format!("</{name}>\n"));
    }

    pub fn finish(self) -> String {
        assert!(self.stack.is_empty(), "unclosed elements");
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaping_and_control_characters() {
        assert_eq!(escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
        assert_eq!(escape("x\u{0}y\u{b}z"), "xyz");
        let mut w = XmlWriter::new();
        w.open("A", &[("k", "v&w")]);
        w.text("T", "1 < 2");
        w.empty("E", &[]);
        w.close();
        let s = w.finish();
        assert!(s.contains("<A k=\"v&amp;w\">"));
        assert!(s.contains("<T>1 &lt; 2</T>"));
        assert!(roxmltree::Document::parse(&s).is_ok());
    }
}
