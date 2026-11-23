#[derive(Default)]
pub(super) struct Captions(Vec<Cue>);
struct Cue {
    start: f64,
    end: f64,
    text: String,
    latest_end: f64,
}

fn time(value: &str) -> Option<f64> {
    let mut parts = value.trim().split(':').rev();
    let seconds = parts.next()?.replace(',', ".").parse::<f64>().ok()?;
    let minutes = parts.next()?.parse::<u32>().ok()?;
    let hours = parts
        .next()
        .map(str::parse::<u32>)
        .transpose()
        .ok()?
        .unwrap_or(0);
    (parts.next().is_none() && seconds.is_finite() && (0.0..60.).contains(&seconds))
        .then_some(hours as f64 * 3600. + minutes as f64 * 60. + seconds)
}
impl Captions {
    pub(super) fn parse(vtt: &str) -> Self {
        let mut cues = Vec::new();
        let normalized = vtt.trim_start_matches('\u{feff}').replace("\r\n", "\n");
        for block in normalized.split("\n\n") {
            let mut lines = block.lines();
            let Some((a, b)) = lines.find_map(|line| line.split_once("-->")) else {
                continue;
            };
            let (Some(start), Some(end)) = (time(a), b.split_whitespace().next().and_then(time))
            else {
                continue;
            };
            if end <= start {
                continue;
            }
            let mut tag = false;
            let text = lines
                .collect::<Vec<_>>()
                .join("\n")
                .chars()
                .filter(|&c| {
                    if c == '<' {
                        tag = true;
                        return false;
                    }
                    if c == '>' && tag {
                        tag = false;
                        return false;
                    }
                    !tag
                })
                .collect::<String>()
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&nbsp;", " ")
                .replace("&amp;", "&");
            cues.push(Cue {
                start,
                end,
                text,
                latest_end: end,
            });
        }
        cues.sort_by(|a, b| a.start.total_cmp(&b.start));
        let mut last_end: f64 = 0.;
        for cue in &mut cues {
            last_end = last_end.max(cue.end);
            cue.latest_end = last_end;
        }
        Self(cues)
    }
    pub(super) fn active(&self, at: f64) -> String {
        let start = self.0.partition_point(|c| c.latest_end <= at);
        self.0[start..]
            .iter()
            .take_while(|c| c.start <= at)
            .filter(|c| at < c.end)
            .map(|c| c.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caption_seek_handles_overlaps_and_gaps() {
        let c = Captions::parse(
            "WEBVTT\r\n\r\n1\r\n00:01.000 --> 00:08.100\r\n<i>A</i>\r\n\r\n00:02.000 --> 00:03.000 align:start\r\nB &amp; C",
        );
        assert_eq!(c.active(2.5), "A\nB & C");
        assert_eq!(c.active(7.), "A");
        assert_eq!(c.active(8.1), "");
        assert_eq!(c.active(0.), "");
    }
}
