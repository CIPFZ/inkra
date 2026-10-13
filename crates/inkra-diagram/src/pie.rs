//! Mermaid `pie` charts: `title`, `showData`, `"label" : value` slices.

use crate::draw::{self, Scene};
use crate::{Error, Palette};

/// A parsed pie chart.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Pie {
    /// Title.
    pub title: Option<String>,
    /// Show values in the legend.
    pub show_data: bool,
    /// Slices.
    pub slices: Vec<(String, f64)>,
}

/// Parses pie source.
pub fn parse(src: &str) -> Result<Pie, Error> {
    let mut p = Pie::default();
    for (ln, raw) in src.lines().enumerate() {
        let line = raw.split("%%").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if ln == 0 || line.starts_with("pie") {
            let rest = line.trim_start_matches("pie").trim();
            if rest.contains("showData") {
                p.show_data = true;
            }
            if let Some(t) = rest.split("title").nth(1) {
                p.title = Some(t.trim().to_owned());
            }
            continue;
        }
        if let Some(t) = line.strip_prefix("title") {
            p.title = Some(t.trim().to_owned());
            continue;
        }
        if line == "showData" {
            p.show_data = true;
            continue;
        }
        let (label, value) = line.rsplit_once(':').ok_or(Error::new(
            ln + 1,
            &format!("expected `\"label\" : value`, got `{line}`"),
        ))?;
        let label = label.trim().trim_matches('"').to_owned();
        let value: f64 = value
            .trim()
            .parse()
            .map_err(|_| Error::new(ln + 1, &format!("`{}` is not a number", value.trim())))?;
        if value < 0.0 {
            return Err(Error::new(ln + 1, "negative slice value"));
        }
        p.slices.push((label, value));
    }
    if p.slices.is_empty() {
        return Err(Error::new(1, "pie chart without slices"));
    }
    Ok(p)
}

/// Draws a pie chart.
pub fn render(p: &Pie, pal: &Palette) -> Scene {
    let font = 14.0;
    let r = 110.0;
    let margin = 20.0;
    let title_h = if p.title.is_some() { 34.0 } else { 0.0 };
    let total: f64 = p
        .slices
        .iter()
        .map(|s| s.1)
        .sum::<f64>()
        .max(f64::MIN_POSITIVE);
    let legend: Vec<String> = p
        .slices
        .iter()
        .map(|(l, v)| {
            if p.show_data {
                format!("{l} [{}]", trim_num(*v))
            } else {
                l.clone()
            }
        })
        .collect();
    let legend_w = legend
        .iter()
        .map(|l| draw::text_width(l, font))
        .fold(0.0, f32::max)
        + 30.0;
    let width = margin * 2.0 + 2.0 * r + 30.0 + legend_w;
    let height = (margin * 2.0 + title_h + 2.0 * r)
        .max(margin * 2.0 + title_h + 22.0 * p.slices.len() as f32);
    let mut sc = Scene {
        width,
        height,
        fills: Vec::new(),
    };
    if let Some(t) = &p.title {
        let tw = draw::text_width(t, font * 1.2);
        draw::draw_text(
            &mut sc,
            t,
            width / 2.0 - tw / 2.0,
            margin + font * 1.2,
            font * 1.2,
            pal.fg,
        );
    }
    let (cx, cy) = (margin + r, margin + title_h + r);
    let mut a0 = -std::f64::consts::FRAC_PI_2;
    for (i, (_, v)) in p.slices.iter().enumerate() {
        let frac = v / total;
        let a1 = a0 + frac * std::f64::consts::TAU;
        let color = pal.pie[i % pal.pie.len()];
        if frac >= 0.9999 {
            sc.fill(draw::ellipse(cx, cy, r, r), color);
        } else if frac > 0.0 {
            let mut pts = vec![(cx, cy)];
            let steps = ((frac * 96.0).ceil() as usize).max(2);
            for k in 0..=steps {
                let a = a0 + (a1 - a0) * k as f64 / steps as f64;
                pts.push((cx + r * a.cos() as f32, cy + r * a.sin() as f32));
            }
            sc.fill(draw::polygon(&pts), color);
            sc.stroke(draw::polygon(&pts), pal.bg, 1.5, None);
        }
        if frac >= 0.04 {
            let mid = (a0 + a1) / 2.0;
            let t = format!("{:.0}%", frac * 100.0);
            let tw = draw::text_width(&t, font * 0.9);
            let (tx, ty) = (
                cx + 0.66 * r * mid.cos() as f32,
                cy + 0.66 * r * mid.sin() as f32,
            );
            draw::draw_text(
                &mut sc,
                &t,
                tx - tw / 2.0,
                ty + font * 0.35,
                font * 0.9,
                pal.pie_text,
            );
        }
        a0 = a1;
    }
    // Legend.
    let lx = cx + r + 30.0;
    let mut ly = cy - 22.0 * p.slices.len() as f32 / 2.0;
    for (i, l) in legend.iter().enumerate() {
        sc.fill(
            draw::rounded_rect(lx, ly + 3.0, 14.0, 14.0, 2.0),
            pal.pie[i % pal.pie.len()],
        );
        draw::draw_text(&mut sc, l, lx + 22.0, ly + 15.0, font, pal.fg);
        ly += 22.0;
    }
    sc
}

fn trim_num(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pies() {
        let p = parse(
            "pie showData\n  title Pets\n  \"Dogs\" : 386\n  \"Cats\" : 85.5\n  \"Rats\" : 15",
        )
        .unwrap();
        assert!(p.show_data);
        assert_eq!(p.title.as_deref(), Some("Pets"));
        assert_eq!(p.slices.len(), 3);
        assert!(parse("pie\n\"x\" : abc").is_err());
        assert!(parse("pie title T").is_err());
    }
}
