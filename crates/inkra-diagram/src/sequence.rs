//! Mermaid `sequenceDiagram` (SPEC 2.4 P1 subset): participants and actors,
//! the six arrow styles (solid/dotted × arrow/open/cross, plus async `)`),
//! notes, `loop`/`alt`/`else`/`opt`/`par`/`and`/`critical`/`break` blocks,
//! activations and `autonumber`.

use std::collections::HashMap;

use crate::draw::{self, Scene};
use crate::{Error, Palette};

/// Arrow kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    /// `->` / `-->`: line without head.
    Line,
    /// `->>` / `-->>`: filled arrow.
    Filled,
    /// `-x` / `--x`: cross.
    Cross,
    /// `-)` / `--)`: open (async) arrow.
    Open,
}

/// A statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A message.
    Message {
        /// Sender.
        from: usize,
        /// Receiver.
        to: usize,
        /// Text.
        text: String,
        /// Dotted line.
        dotted: bool,
        /// Head.
        arrow: Arrow,
        /// Activate the receiver (`+`) / deactivate the sender (`-`).
        activate: Option<bool>,
    },
    /// A note.
    Note {
        /// Participants covered (one or two).
        over: Vec<usize>,
        /// `left of` / `right of` / `over`.
        side: Side,
        /// Text.
        text: String,
    },
    /// Start of a block (`loop`, `alt`, …).
    Open {
        /// Keyword.
        kind: String,
        /// Label.
        label: String,
    },
    /// A divider inside a block (`else`, `and`).
    Divider {
        /// Label.
        label: String,
    },
    /// End of a block.
    Close,
    /// `activate` / `deactivate`.
    Activation {
        /// Participant.
        who: usize,
        /// On.
        on: bool,
    },
}

/// Note placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// `left of`.
    Left,
    /// `right of`.
    Right,
    /// `over`.
    Over,
}

/// A participant.
#[derive(Debug, Clone, PartialEq)]
pub struct Participant {
    /// Id.
    pub id: String,
    /// Display name.
    pub label: String,
    /// `actor` (stick figure).
    pub actor: bool,
}

/// A parsed sequence diagram.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Seq {
    /// Participants in order.
    pub participants: Vec<Participant>,
    /// Statements.
    pub items: Vec<Item>,
    /// `autonumber`.
    pub autonumber: bool,
    /// `title`.
    pub title: Option<String>,
}

fn arrow_of(op: &str) -> Option<(bool, Arrow)> {
    Some(match op {
        "->" => (false, Arrow::Line),
        "-->" => (true, Arrow::Line),
        "->>" => (false, Arrow::Filled),
        "-->>" => (true, Arrow::Filled),
        "-x" => (false, Arrow::Cross),
        "--x" => (true, Arrow::Cross),
        "-)" => (false, Arrow::Open),
        "--)" => (true, Arrow::Open),
        "<<->>" => (false, Arrow::Filled),
        "<<-->>" => (true, Arrow::Filled),
        _ => return None,
    })
}

/// Parses sequence diagram source.
pub fn parse(src: &str) -> Result<Seq, Error> {
    let mut s = Seq::default();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut get = |s: &mut Seq, id: &str| -> usize {
        let id = id.trim();
        if let Some(&i) = index.get(id) {
            return i;
        }
        s.participants.push(Participant {
            id: id.to_owned(),
            label: id.to_owned(),
            actor: false,
        });
        index.insert(id.to_owned(), s.participants.len() - 1);
        s.participants.len() - 1
    };
    let mut depth = 0i32;
    for (ln, raw) in src.lines().enumerate().skip(1) {
        let line_no = ln + 1;
        let line = raw.split("%%").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let low = line.to_ascii_lowercase();
        let word = low.split_whitespace().next().unwrap_or("");
        match word {
            "participant" | "actor" => {
                let rest = line[word.len()..].trim();
                let (id, label) = match rest.split_once(" as ") {
                    Some((a, b)) => (a.trim(), b.trim()),
                    None => (rest, rest),
                };
                let i = get(&mut s, id);
                s.participants[i].label = label.to_owned();
                s.participants[i].actor = word == "actor";
            }
            "autonumber" => s.autonumber = true,
            "title" => s.title = Some(line[5..].trim().trim_start_matches(':').trim().to_owned()),
            "activate" | "deactivate" => {
                let who = get(&mut s, line[word.len()..].trim());
                s.items.push(Item::Activation {
                    who,
                    on: word == "activate",
                });
            }
            "note" => {
                let rest = line[4..].trim();
                let (head, text) = rest
                    .split_once(':')
                    .ok_or(Error::new(line_no, "note needs `:`"))?;
                let head = head.trim();
                // ASCII lowercasing keeps byte offsets, so `head` can be sliced.
                let hl = head.to_ascii_lowercase();
                let (side, who) = if hl.starts_with("left of") {
                    (Side::Left, &head["left of".len()..])
                } else if hl.starts_with("right of") {
                    (Side::Right, &head["right of".len()..])
                } else if hl.starts_with("over") {
                    (Side::Over, &head["over".len()..])
                } else {
                    return Err(Error::new(
                        line_no,
                        "note: expected left of / right of / over",
                    ));
                };
                let over: Vec<usize> = who.split(',').map(|w| get(&mut s, w)).collect();
                s.items.push(Item::Note {
                    over,
                    side,
                    text: text.trim().to_owned(),
                });
            }
            "loop" | "alt" | "opt" | "par" | "critical" | "break" | "rect" => {
                depth += 1;
                s.items.push(Item::Open {
                    kind: word.to_owned(),
                    label: line[word.len()..].trim().to_owned(),
                });
            }
            "else" | "and" | "option" => s.items.push(Item::Divider {
                label: line[word.len()..].trim().to_owned(),
            }),
            "end" => {
                depth -= 1;
                if depth < 0 {
                    return Err(Error::new(line_no, "`end` without a block"));
                }
                s.items.push(Item::Close);
            }
            "box" | "link" | "links" | "create" | "destroy" => {}
            _ => {
                // Message: `A->>B: text`, with optional `+`/`-` before the target.
                let (lhs, text) = line.split_once(':').unwrap_or((line, ""));
                let ops = [
                    "<<-->>", "<<->>", "-->>", "->>", "-->", "->", "--x", "-x", "--)", "-)",
                ];
                let Some((pos, op)) = ops
                    .iter()
                    .filter_map(|op| lhs.find(op).map(|p| (p, *op)))
                    .min_by_key(|(p, op)| (*p, std::cmp::Reverse(op.len())))
                else {
                    return Err(Error::new(line_no, &format!("cannot parse `{line}`")));
                };
                let from_id = lhs[..pos].trim();
                let mut to_id = lhs[pos + op.len()..].trim();
                let mut activate = None;
                if let Some(t) = to_id.strip_prefix('+') {
                    activate = Some(true);
                    to_id = t.trim();
                } else if let Some(t) = to_id.strip_prefix('-') {
                    activate = Some(false);
                    to_id = t.trim();
                }
                if from_id.is_empty() || to_id.is_empty() {
                    return Err(Error::new(line_no, &format!("cannot parse `{line}`")));
                }
                let (dotted, arrow) = arrow_of(op).unwrap_or((false, Arrow::Filled));
                let from = get(&mut s, from_id);
                let to = get(&mut s, to_id);
                s.items.push(Item::Message {
                    from,
                    to,
                    text: text.trim().to_owned(),
                    dotted,
                    arrow,
                    activate,
                });
            }
        }
    }
    if depth != 0 {
        return Err(Error::new(
            src.lines().count(),
            "unclosed block (missing `end`)",
        ));
    }
    Ok(s)
}

const FONT: f32 = 14.0;

/// Lays out and draws a sequence diagram.
pub fn render(s: &Seq, pal: &Palette) -> Scene {
    let n = s.participants.len().max(1);
    let box_h = 38.0;
    let labels: Vec<Vec<String>> = s
        .participants
        .iter()
        .map(|p| draw::wrap(&p.label, FONT, 160.0))
        .collect();
    let box_w: Vec<f32> = labels
        .iter()
        .map(|l| draw::lines_size(l, FONT).0 + 30.0)
        .map(|w| w.max(80.0))
        .collect();
    // Column gaps must fit the messages between neighbours.
    let mut gap = vec![50.0f32; n];
    for it in &s.items {
        if let Item::Message { from, to, text, .. } = it {
            let (a, b) = ((*from).min(*to), (*from).max(*to));
            let tw = draw::text_width(text, FONT * 0.95) + 30.0;
            if a == b {
                gap[a] = gap[a].max(tw.min(220.0) + 30.0);
            } else {
                let span = b - a;
                for g in gap.iter_mut().take(b).skip(a) {
                    *g = g.max(tw / span as f32);
                }
            }
        }
        if let Item::Note { over, side, text } = it {
            let tw = draw::text_width(text, FONT * 0.9).min(180.0) + 24.0;
            if over.len() == 1 && *side != Side::Over {
                let i = over[0];
                match side {
                    Side::Right => gap[i] = gap[i].max(tw + 10.0),
                    _ if i > 0 => gap[i - 1] = gap[i - 1].max(tw + 10.0),
                    _ => {}
                }
            }
        }
    }
    let margin = 20.0;
    let mut cx = vec![0.0f32; n];
    let mut x = margin;
    for i in 0..n {
        let w = box_w.get(i).copied().unwrap_or(80.0);
        cx[i] = x + w / 2.0;
        let next_w = box_w.get(i + 1).copied().unwrap_or(0.0);
        x += w / 2.0 + gap[i].max(w / 2.0 + next_w / 2.0 + 20.0) - next_w / 2.0;
        if i + 1 == n {
            x = cx[i] + w / 2.0;
        }
    }
    // Leave room for notes right of the last participant.
    let width = x
        + margin
        + s.items
            .iter()
            .filter_map(|it| match it {
                Item::Note {
                    over,
                    side: Side::Right,
                    text,
                } if over.first() == Some(&(n - 1)) => {
                    Some(draw::text_width(text, FONT * 0.9).min(180.0) + 30.0)
                }
                Item::Message { from, to, .. } if from == to && *from == n - 1 => Some(60.0),
                _ => None,
            })
            .fold(0.0, f32::max);
    let mut sc = Scene {
        width,
        height: 0.0,
        fills: Vec::new(),
    };
    let title_h = if let Some(t) = &s.title {
        let tw = draw::text_width(t, FONT * 1.15);
        draw::draw_text(
            &mut sc,
            t,
            width / 2.0 - tw / 2.0,
            margin + FONT,
            FONT * 1.15,
            pal.fg,
        );
        FONT * 2.0
    } else {
        0.0
    };
    let top = margin + title_h;
    // Body: compute rows.
    let mut y = top + box_h + 24.0;
    let mut active: Vec<Vec<f32>> = vec![Vec::new(); n];
    let mut bars: Vec<(usize, f32, f32)> = Vec::new();
    // (kind, label, top, dividers) while open; (kind, label, top, bottom, dividers) when done.
    type Dividers = Vec<(f32, String)>;
    let mut blocks: Vec<(String, String, f32, Dividers)> = Vec::new();
    let mut finished_blocks: Vec<(String, String, f32, f32, Dividers)> = Vec::new();
    let mut number = 0;
    let mut later: Scene = Scene::default();
    for it in &s.items {
        match it {
            Item::Message {
                from,
                to,
                text,
                dotted,
                arrow,
                activate,
            } => {
                let mut label = text.clone();
                if s.autonumber {
                    number += 1;
                    label = format!("{number}. {label}");
                }
                let lines = draw::wrap(&label, FONT * 0.95, 260.0);
                let (_, th) = draw::lines_size(&lines, FONT * 0.95);
                y += th;
                let off = |i: usize| -> f32 {
                    if active[i].is_empty() {
                        0.0
                    } else {
                        5.0 * active[i].len() as f32
                    }
                };
                let x1 = cx[*from] + if from < to { off(*from) } else { -off(*from) };
                let x2 = cx[*to]
                    - if from < to {
                        off(*to).max(0.0)
                    } else {
                        -off(*to)
                    };
                let dash = dotted.then_some((4.0, 3.0));
                if from == to {
                    let x0 = cx[*from] + off(*from);
                    let pts = [
                        (x0, y),
                        (x0 + 40.0, y),
                        (x0 + 40.0, y + 18.0),
                        (x0 + 2.0, y + 18.0),
                    ];
                    later.stroke(draw::polyline(&pts), pal.edge, 1.3, dash);
                    draw::draw_lines_centered(
                        &mut later,
                        &lines,
                        x0 + 20.0 + draw::lines_size(&lines, FONT * 0.95).0 / 2.0 + 24.0,
                        y - th / 2.0 - 2.0,
                        FONT * 0.95,
                        pal.fg,
                    );
                    head(
                        &mut later,
                        *arrow,
                        (x0 + 40.0, y + 18.0),
                        (x0 + 2.0, y + 18.0),
                        pal,
                    );
                    y += 18.0;
                } else {
                    let tip = (x2, y);
                    let shaft_end = match arrow {
                        Arrow::Filled => (x2 - (x2 - x1).signum() * 7.0, y),
                        _ => tip,
                    };
                    later.stroke(draw::polyline(&[(x1, y), shaft_end]), pal.edge, 1.3, dash);
                    head(&mut later, *arrow, (x1, y), tip, pal);
                    draw::draw_lines_centered(
                        &mut later,
                        &lines,
                        (x1 + x2) / 2.0,
                        y - th / 2.0 - 3.0,
                        FONT * 0.95,
                        pal.fg,
                    );
                }
                match activate {
                    Some(true) => active[*to].push(y),
                    Some(false) => {
                        if let Some(st) = active[*from].pop() {
                            bars.push((*from, st, y));
                        }
                    }
                    None => {}
                }
                y += 22.0;
            }
            Item::Note { over, side, text } => {
                let lines = draw::wrap(text, FONT * 0.9, 180.0);
                let (tw, th) = draw::lines_size(&lines, FONT * 0.9);
                let (x0, w) = match side {
                    Side::Over => {
                        let a = over.iter().map(|&i| cx[i]).fold(f32::INFINITY, f32::min);
                        let b = over
                            .iter()
                            .map(|&i| cx[i])
                            .fold(f32::NEG_INFINITY, f32::max);
                        let w = (b - a + 40.0).max(tw + 20.0);
                        ((a + b) / 2.0 - w / 2.0, w)
                    }
                    Side::Right => (cx[over[0]] + 12.0, tw + 20.0),
                    Side::Left => (cx[over[0]] - 12.0 - tw - 20.0, tw + 20.0),
                };
                let h = th + 12.0;
                later.fill(draw::rounded_rect(x0, y, w, h, 2.0), pal.note_fill);
                later.stroke(
                    draw::rounded_rect(x0, y, w, h, 2.0),
                    pal.note_stroke,
                    1.0,
                    None,
                );
                draw::draw_lines_centered(
                    &mut later,
                    &lines,
                    x0 + w / 2.0,
                    y + h / 2.0,
                    FONT * 0.9,
                    pal.fg,
                );
                y += h + 14.0;
            }
            Item::Open { kind, label } => {
                y += 6.0;
                blocks.push((kind.clone(), label.clone(), y, Vec::new()));
                y += 26.0;
            }
            Item::Divider { label } => {
                if let Some(b) = blocks.last_mut() {
                    b.3.push((y, label.clone()));
                }
                y += 26.0;
            }
            Item::Close => {
                if let Some((k, l, start, divs)) = blocks.pop() {
                    finished_blocks.push((k, l, start, y, divs));
                }
                y += 10.0;
            }
            Item::Activation { who, on } => {
                if *on {
                    active[*who].push(y - 10.0);
                } else if let Some(st) = active[*who].pop() {
                    bars.push((*who, st, y - 10.0));
                }
            }
        }
    }
    let bottom = y + 10.0;
    for (i, st) in active.iter().enumerate() {
        for &a in st {
            bars.push((i, a, bottom));
        }
    }
    // Lifelines.
    for &c in cx.iter().take(s.participants.len()) {
        sc.stroke(
            draw::polyline(&[(c, top + box_h), (c, bottom)]),
            pal.lifeline,
            1.0,
            Some((3.0, 3.0)),
        );
    }
    // Blocks (behind messages; their labels go on top of activation bars).
    let mut block_labels = Scene::default();
    for (kind, label, y0, y1, divs) in &finished_blocks {
        let x0 = margin * 0.5;
        let x1 = width - margin * 0.5;
        sc.fill(
            draw::rounded_rect(x0, *y0, x1 - x0, y1 - y0, 3.0),
            pal.cluster_fill,
        );
        sc.stroke(
            draw::rounded_rect(x0, *y0, x1 - x0, y1 - y0, 3.0),
            pal.cluster_stroke,
            1.0,
            None,
        );
        let tag_w = draw::text_width(kind, FONT * 0.8) + 14.0;
        sc.fill(
            draw::polygon(&[
                (x0, *y0),
                (x0 + tag_w, *y0),
                (x0 + tag_w, *y0 + 14.0),
                (x0 + tag_w - 6.0, *y0 + 20.0),
                (x0, *y0 + 20.0),
            ]),
            pal.cluster_stroke,
        );
        draw::draw_text(&mut sc, kind, x0 + 6.0, *y0 + 14.5, FONT * 0.8, pal.bg);
        if !label.is_empty() {
            let t = format!("[{label}]");
            let tw = draw::text_width(&t, FONT * 0.85);
            block_labels.fill(
                draw::rounded_rect(x0 + tag_w + 6.0, *y0 + 2.0, tw + 8.0, 18.0, 2.0),
                pal.cluster_fill,
            );
            draw::draw_text(
                &mut block_labels,
                &t,
                x0 + tag_w + 10.0,
                *y0 + 15.0,
                FONT * 0.85,
                pal.fg,
            );
        }
        for (dy, l) in divs {
            sc.stroke(
                draw::polyline(&[(x0, *dy), (x1, *dy)]),
                pal.cluster_stroke,
                1.0,
                Some((4.0, 3.0)),
            );
            if !l.is_empty() {
                let t = format!("[{l}]");
                let tw = draw::text_width(&t, FONT * 0.85);
                block_labels.fill(
                    draw::rounded_rect(x0 + 6.0, dy + 2.0, tw + 8.0, 18.0, 2.0),
                    pal.cluster_fill,
                );
                draw::draw_text(
                    &mut block_labels,
                    &t,
                    x0 + 10.0,
                    dy + 15.0,
                    FONT * 0.85,
                    pal.fg,
                );
            }
        }
    }
    // Activation bars.
    for (i, a, b) in bars {
        sc.fill(
            draw::rounded_rect(cx[i] - 5.0, a, 10.0, (b - a).max(8.0), 1.0),
            pal.node_fill,
        );
        sc.stroke(
            draw::rounded_rect(cx[i] - 5.0, a, 10.0, (b - a).max(8.0), 1.0),
            pal.node_stroke,
            1.0,
            None,
        );
    }
    sc.fills.extend(later.fills);
    sc.fills.extend(block_labels.fills);
    // Participant boxes top and bottom.
    for (i, p) in s.participants.iter().enumerate() {
        for yb in [top, bottom] {
            let w = box_w[i];
            if p.actor {
                actor(&mut sc, cx[i], yb, pal);
                let tw = draw::text_width(&p.label, FONT);
                draw::draw_text(
                    &mut sc,
                    &p.label,
                    cx[i] - tw / 2.0,
                    yb + box_h + 4.0,
                    FONT,
                    pal.fg,
                );
            } else {
                sc.fill(
                    draw::rounded_rect(cx[i] - w / 2.0, yb, w, box_h, 3.0),
                    pal.node_fill,
                );
                sc.stroke(
                    draw::rounded_rect(cx[i] - w / 2.0, yb, w, box_h, 3.0),
                    pal.node_stroke,
                    1.3,
                    None,
                );
                draw::draw_lines_centered(
                    &mut sc,
                    &labels[i],
                    cx[i],
                    yb + box_h / 2.0,
                    FONT,
                    pal.fg,
                );
            }
        }
    }
    sc.height = bottom
        + box_h
        + margin
        + if s.participants.iter().any(|p| p.actor) {
            20.0
        } else {
            0.0
        };
    sc
}

fn actor(sc: &mut Scene, cx: f32, y: f32, pal: &Palette) {
    let c = pal.node_stroke;
    sc.stroke(draw::ellipse(cx, y + 6.0, 6.0, 6.0), c, 1.4, None);
    sc.stroke(
        draw::polyline(&[(cx, y + 12.0), (cx, y + 26.0)]),
        c,
        1.4,
        None,
    );
    sc.stroke(
        draw::polyline(&[(cx - 10.0, y + 17.0), (cx + 10.0, y + 17.0)]),
        c,
        1.4,
        None,
    );
    sc.stroke(
        draw::polyline(&[(cx - 8.0, y + 36.0), (cx, y + 26.0), (cx + 8.0, y + 36.0)]),
        c,
        1.4,
        None,
    );
}

fn head(sc: &mut Scene, a: Arrow, from: (f32, f32), tip: (f32, f32), pal: &Palette) {
    match a {
        Arrow::Line => {}
        Arrow::Filled => sc.fill(draw::arrowhead(from, tip, 9.0), pal.edge),
        Arrow::Open => {
            let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
            let l = (dx * dx + dy * dy).sqrt().max(0.01);
            let (ux, uy) = (dx / l, dy / l);
            let b = (tip.0 - ux * 9.0, tip.1 - uy * 9.0);
            sc.stroke(
                draw::polyline(&[(b.0 - uy * 5.0, b.1 + ux * 5.0), tip]),
                pal.edge,
                1.3,
                None,
            );
        }
        Arrow::Cross => {
            let d = 4.5;
            let c = (tip.0 - (tip.0 - from.0).signum() * 5.0, tip.1);
            sc.stroke(
                draw::polyline(&[(c.0 - d, c.1 - d), (c.0 + d, c.1 + d)]),
                pal.edge,
                1.6,
                None,
            );
            sc.stroke(
                draw::polyline(&[(c.0 - d, c.1 + d), (c.0 + d, c.1 - d)]),
                pal.edge,
                1.6,
                None,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_messages_notes_blocks() {
        let s = parse(
            "sequenceDiagram\n  participant A as Alice\n  actor B as Bob\n  A->>+B: Hello\n  B-->>-A: Hi\n  loop Every minute\n    A-)B: ping\n  end\n  alt ok\n    B-xA: no\n  else not ok\n    B--)A: later\n  end\n  Note right of B: thinking\n  Note over A,B: shared",
        )
        .unwrap();
        assert_eq!(s.participants.len(), 2);
        assert!(s.participants[1].actor);
        assert_eq!(s.participants[0].label, "Alice");
        let msgs: Vec<_> = s
            .items
            .iter()
            .filter(|i| matches!(i, Item::Message { .. }))
            .collect();
        assert_eq!(msgs.len(), 5);
        assert!(matches!(
            msgs[0],
            Item::Message {
                arrow: Arrow::Filled,
                dotted: false,
                activate: Some(true),
                ..
            }
        ));
        assert!(matches!(
            msgs[1],
            Item::Message {
                dotted: true,
                activate: Some(false),
                ..
            }
        ));
        assert!(matches!(
            msgs[2],
            Item::Message {
                arrow: Arrow::Open,
                ..
            }
        ));
        assert!(matches!(
            msgs[3],
            Item::Message {
                arrow: Arrow::Cross,
                ..
            }
        ));
        assert!(s.items.iter().any(|i| matches!(
            i,
            Item::Note {
                side: Side::Over,
                ..
            }
        )));
        assert!(parse("sequenceDiagram\n loop x\n A->>B: y").is_err());
    }
}
