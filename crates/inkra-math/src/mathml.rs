//! MathML presentation markup of a formula (EX-001 "公式输出为 MathML（可选）").

use std::fmt::Write;

use crate::parse::{ArrayKind, ColAlign, MathStyle, Node, Parsed, parse};
use crate::symbols::{Class, Variant};

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn variant_attr(v: Variant) -> &'static str {
    match v {
        Variant::Normal | Variant::Italic => "",
        Variant::Upright => " mathvariant=\"normal\"",
        Variant::Bold => " mathvariant=\"bold\"",
        Variant::BoldItalic => " mathvariant=\"bold-italic\"",
        Variant::DoubleStruck => " mathvariant=\"double-struck\"",
        Variant::Script => " mathvariant=\"script\"",
        Variant::Fraktur => " mathvariant=\"fraktur\"",
        Variant::SansSerif => " mathvariant=\"sans-serif\"",
        _ => " mathvariant=\"monospace\"",
    }
}

fn row(list: &[Node], out: &mut String) {
    if list.len() == 1 {
        node(&list[0], out);
    } else {
        out.push_str("<mrow>");
        for n in list {
            node(n, out);
        }
        out.push_str("</mrow>");
    }
}

fn delim(c: Option<char>, out: &mut String) {
    if let Some(c) = c {
        let _ = write!(
            out,
            "<mo fence=\"true\" stretchy=\"true\">{}</mo>",
            esc(&c.to_string())
        );
    }
}

fn node(n: &Node, out: &mut String) {
    match n {
        Node::Sym { ch, class, variant } => {
            let s = esc(&ch.to_string());
            match class {
                Class::Ord if ch.is_ascii_digit() || *ch == '.' => {
                    let _ = write!(out, "<mn>{s}</mn>");
                }
                Class::Ord if ch.is_alphabetic() => {
                    // Single letters are italic by default in <mi>.
                    let _ = write!(out, "<mi{}>{s}</mi>", variant_attr(*variant));
                }
                Class::Ord => {
                    let _ = write!(out, "<mi mathvariant=\"normal\">{s}</mi>");
                }
                Class::Open | Class::Close => {
                    let _ = write!(out, "<mo stretchy=\"false\">{s}</mo>");
                }
                _ => {
                    let _ = write!(out, "<mo>{s}</mo>");
                }
            }
        }
        Node::BigOp {
            ch,
            limits,
            default_limits,
        } => {
            let movable = if limits.unwrap_or(*default_limits) {
                "true"
            } else {
                "false"
            };
            let _ = write!(
                out,
                "<mo largeop=\"true\" movablelimits=\"{movable}\">{}</mo>",
                esc(&ch.to_string())
            );
        }
        Node::OpName { name, .. } => {
            let _ = write!(
                out,
                "<mi mathvariant=\"normal\">{}</mi><mo>&#x2061;</mo>",
                esc(name)
            );
        }
        Node::Text { text, bold, italic } => {
            let v = match (bold, italic) {
                (true, true) => " mathvariant=\"bold-italic\"",
                (true, false) => " mathvariant=\"bold\"",
                (false, true) => " mathvariant=\"italic\"",
                _ => "",
            };
            let _ = write!(out, "<mtext{v}>{}</mtext>", esc(text));
        }
        Node::Group(list) => row(list, out),
        Node::Scripts { base, sup, sub } => {
            let under_over = matches!(&**base, Node::BigOp { limits, default_limits, .. } if limits.unwrap_or(*default_limits))
                || matches!(&**base, Node::Brace { .. })
                || matches!(
                    &**base,
                    Node::OpName {
                        limits: Some(true),
                        ..
                    }
                );
            let (tag, sub_tag, sup_tag) = if under_over {
                ("munderover", "munder", "mover")
            } else {
                ("msubsup", "msub", "msup")
            };
            match (sub, sup) {
                (Some(b), Some(p)) => {
                    let _ = write!(out, "<{tag}>");
                    node(base, out);
                    row(b, out);
                    row(p, out);
                    let _ = write!(out, "</{tag}>");
                }
                (Some(b), None) => {
                    let _ = write!(out, "<{sub_tag}>");
                    node(base, out);
                    row(b, out);
                    let _ = write!(out, "</{sub_tag}>");
                }
                (None, Some(p)) => {
                    let _ = write!(out, "<{sup_tag}>");
                    node(base, out);
                    row(p, out);
                    let _ = write!(out, "</{sup_tag}>");
                }
                (None, None) => node(base, out),
            }
        }
        Node::Frac {
            num,
            den,
            rule,
            style,
            delims,
        } => {
            if let Some((l, _)) = delims {
                out.push_str("<mrow>");
                delim(Some(*l), out);
            }
            let style_attr = match style {
                Some(MathStyle::Display) => " displaystyle=\"true\"",
                Some(MathStyle::Text) => " displaystyle=\"false\"",
                _ => "",
            };
            let lt = if *rule { "" } else { " linethickness=\"0\"" };
            let _ = write!(out, "<mfrac{lt}{style_attr}>");
            row(num, out);
            row(den, out);
            out.push_str("</mfrac>");
            if let Some((_, r)) = delims {
                delim(Some(*r), out);
                out.push_str("</mrow>");
            }
        }
        Node::Sqrt { index, body } => match index {
            Some(i) => {
                out.push_str("<mroot>");
                row(body, out);
                row(i, out);
                out.push_str("</mroot>");
            }
            None => {
                out.push_str("<msqrt>");
                for n in body {
                    node(n, out);
                }
                out.push_str("</msqrt>");
            }
        },
        Node::LeftRight { left, body, right } => {
            out.push_str("<mrow>");
            delim(*left, out);
            for n in body {
                node(n, out);
            }
            delim(*right, out);
            out.push_str("</mrow>");
        }
        Node::Middle(c) => delim(*c, out),
        Node::Big { ch, size, .. } => {
            if let Some(c) = ch {
                let em = [1.2, 1.8, 2.4, 3.0][usize::from((*size).clamp(1, 4)) - 1];
                let _ = write!(
                    out,
                    "<mo stretchy=\"true\" minsize=\"{em}em\" maxsize=\"{em}em\">{}</mo>",
                    esc(&c.to_string())
                );
            }
        }
        Node::Accent {
            ch,
            body,
            stretchy,
            under,
        } => {
            let tag = if *under { "munder" } else { "mover" };
            let attr = if *under { "accentunder" } else { "accent" };
            let _ = write!(out, "<{tag} {attr}=\"true\">");
            row(body, out);
            let _ = write!(
                out,
                "<mo stretchy=\"{stretchy}\">{}</mo></{tag}>",
                esc(&ch.to_string())
            );
        }
        Node::Overline(b) => {
            out.push_str("<mover accent=\"true\">");
            row(b, out);
            out.push_str("<mo stretchy=\"true\">&#x203E;</mo></mover>");
        }
        Node::Underline(b) => {
            out.push_str("<munder accentunder=\"true\">");
            row(b, out);
            out.push_str("<mo stretchy=\"true\">&#x5F;</mo></munder>");
        }
        Node::Brace { body, over } => {
            let (tag, ch) = if *over {
                ("mover", "&#x23DE;")
            } else {
                ("munder", "&#x23DF;")
            };
            let _ = write!(out, "<{tag}>");
            row(body, out);
            let _ = write!(out, "<mo stretchy=\"true\">{ch}</mo></{tag}>");
        }
        Node::Stack {
            base, over, under, ..
        } => match (over, under) {
            (Some(o), Some(u)) => {
                out.push_str("<munderover>");
                row(base, out);
                row(u, out);
                row(o, out);
                out.push_str("</munderover>");
            }
            (Some(o), None) => {
                out.push_str("<mover>");
                row(base, out);
                row(o, out);
                out.push_str("</mover>");
            }
            (None, Some(u)) => {
                out.push_str("<munder>");
                row(base, out);
                row(u, out);
                out.push_str("</munder>");
            }
            (None, None) => row(base, out),
        },
        Node::Array {
            kind,
            rows,
            cols,
            delims,
            ..
        } => {
            out.push_str("<mrow>");
            delim(delims.0, out);
            let align = |c: usize| -> &'static str {
                match kind {
                    ArrayKind::Cases => "left",
                    ArrayKind::Aligned => {
                        if c.is_multiple_of(2) {
                            "right"
                        } else {
                            "left"
                        }
                    }
                    ArrayKind::Array => match cols.get(c) {
                        Some(ColAlign::Left) => "left",
                        Some(ColAlign::Right) => "right",
                        _ => "center",
                    },
                    _ => "center",
                }
            };
            out.push_str("<mtable>");
            for r in rows {
                out.push_str("<mtr>");
                for (c, cell) in r.iter().enumerate() {
                    let _ = write!(out, "<mtd columnalign=\"{}\">", align(c));
                    row(cell, out);
                    out.push_str("</mtd>");
                }
                out.push_str("</mtr>");
            }
            out.push_str("</mtable>");
            delim(delims.1, out);
            out.push_str("</mrow>");
        }
        Node::Space(mu) => {
            let _ = write!(out, "<mspace width=\"{:.3}em\"/>", mu / 18.0);
        }
        Node::SpaceEm(em) => {
            let _ = write!(out, "<mspace width=\"{em:.3}em\"/>");
        }
        Node::Color {
            rgb: [r, g, b],
            body,
        } => {
            let _ = write!(out, "<mstyle mathcolor=\"#{r:02x}{g:02x}{b:02x}\">");
            row(body, out);
            out.push_str("</mstyle>");
        }
        Node::Boxed(b) => {
            out.push_str("<menclose notation=\"box\">");
            row(b, out);
            out.push_str("</menclose>");
        }
        Node::Style(_) => {}
        Node::Class { body, .. } => row(body, out),
        Node::Phantom { body, .. } => {
            out.push_str("<mphantom>");
            row(body, out);
            out.push_str("</mphantom>");
        }
        Node::Not(inner) => {
            out.push_str("<menclose notation=\"updiagonalstrike\">");
            node(inner, out);
            out.push_str("</menclose>");
        }
        Node::Error(s) => {
            let _ = write!(out, "<merror><mtext>{}</mtext></merror>", esc(s));
        }
    }
}

/// MathML for `tex`; `display` makes a block formula. The TeX source is kept
/// as an annotation for copy and paste.
pub fn to_mathml(tex: &str, display: bool) -> String {
    let p: Parsed = parse(tex);
    let mut body = String::new();
    row(&p.list, &mut body);
    let mut out = format!(
        "<math xmlns=\"http://www.w3.org/1998/Math/MathML\"{}><semantics>",
        if display { " display=\"block\"" } else { "" }
    );
    if let Some((tag, raw)) = &p.tag {
        // Equation tags sit at the right of the row.
        let t = if *raw {
            tag.clone()
        } else {
            format!("({tag})")
        };
        let _ = write!(
            out,
            "<mtable><mlabeledtr><mtd><mtext>{}</mtext></mtd><mtd>{body}</mtd></mlabeledtr></mtable>",
            esc(&t)
        );
    } else {
        out.push_str(&body);
    }
    let _ = write!(
        out,
        "<annotation encoding=\"application/x-tex\">{}</annotation></semantics></math>",
        esc(tex)
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_constructs() {
        let m = to_mathml(r"\frac{a}{b} + \sqrt[3]{x^2} = \sum_{i=1}^{n} i", true);
        assert!(
            m.starts_with("<math xmlns=\"http://www.w3.org/1998/Math/MathML\" display=\"block\">")
        );
        for part in [
            "<mfrac>",
            "<mroot>",
            "<msup>",
            "<munderover>",
            "<mo largeop=\"true\"",
            "<mn>1</mn>",
            "annotation",
        ] {
            assert!(m.contains(part), "{part} in {m}");
        }
        let m = to_mathml(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}", false);
        assert!(m.contains("<mtable>") && m.matches("<mtr>").count() == 2);
        assert!(to_mathml(r"\sin x", false).contains("<mi mathvariant=\"normal\">sin</mi>"));
        assert!(to_mathml(r"\badcmd", false).contains("<merror>"));
        // Every construct of the corpus produces well-formed tag nesting.
        let corpus = include_str!("../../../docs/acceptance/math-corpus.md");
        let mut n = 0;
        for tex in corpus.lines().filter_map(|l| l.strip_prefix("- ")) {
            let m = to_mathml(tex.trim(), true);
            for tag in ["mrow", "mfrac", "msup", "msub", "mtable", "mover", "munder"] {
                assert_eq!(
                    m.matches(&format!("<{tag}>")).count() + m.matches(&format!("<{tag} ")).count(),
                    m.matches(&format!("</{tag}>")).count(),
                    "{tag} in {tex}"
                );
            }
            n += 1;
        }
        assert!(n > 200);
    }
}
