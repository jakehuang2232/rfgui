use rfgui::view::viewport::RendererTestDiagnostics;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct Samples {
    phases: BTreeMap<&'static str, Vec<f64>>,
    exclusive: BTreeMap<&'static str, Vec<f64>>,
    counts: BTreeMap<&'static str, usize>,
    samples: usize,
}
impl Samples {
    pub fn push(&mut self, d: RendererTestDiagnostics) {
        self.samples += 1;
        for (name, ms) in d.phases_ms {
            assert!(ms.is_finite() && ms >= 0.0);
            self.phases.entry(name).or_default().push(ms);
        }
        for (name, ms) in d.exclusive_ms {
            assert!(ms.is_finite() && ms >= 0.0);
            self.exclusive.entry(name).or_default().push(ms);
        }
        for (name, n) in d.counts {
            *self.counts.entry(name).or_default() += n;
        }
    }
    pub fn print(self, mode: &str, case: &str, rows: usize) {
        if self.samples == 0 {
            return;
        }
        let phases = self
            .phases
            .into_iter()
            .map(|(k, v)| (k, super::p50(v)))
            .collect::<BTreeMap<_, _>>();
        let exclusive = self
            .exclusive
            .into_iter()
            .map(|(k, v)| (k, super::p50(v)))
            .collect::<BTreeMap<_, _>>();
        let counts = self
            .counts
            .into_iter()
            .map(|(k, v)| (k, v as f64 / self.samples as f64))
            .collect::<BTreeMap<_, _>>();
        println!(
            "f0 mode={mode} case={case} rows={rows} phases_p50_ms={phases:?} exclusive_p50_ms={exclusive:?} counts_mean={counts:?}"
        );
    }
}
