use super::*;

impl TopologySignature {
    /// Diagnostic only. Reuse still requires complete canonical equality.
    pub(super) fn differences(&self, current: &Self) -> Vec<String> {
        let mut result = Vec::new();
        macro_rules! compare {
            ($field:ident) => {
                if self.$field != current.$field {
                    let first = self
                        .$field
                        .iter()
                        .zip(&current.$field)
                        .position(|(a, b)| a != b);
                    result.push(format!(
                        "{} len={}->{} first={first:?}",
                        stringify!($field),
                        self.$field.len(),
                        current.$field.len()
                    ));
                }
            };
        }
        compare!(passes);
        compare!(textures);
        compare!(texture_metadata);
        compare!(buffers);
        compare!(buffer_metadata);
        compare!(external_sinks);
        compare!(texture_attachment_pairs);
        if let Some((index, (old, new))) = self
            .passes
            .iter()
            .zip(&current.passes)
            .enumerate()
            .find(|(_, (a, b))| a != b)
        {
            result.push(format!(
                "pass[{index}] name={}->{} kind={} details={} usages={}",
                old.name,
                new.name,
                old.kind != new.kind,
                old.details != new.details,
                old.usages != new.usages
            ));
            if old.details != new.details {
                result.push(format!(
                    "pass[{index}] details {:?} -> {:?}",
                    old.details, new.details
                ));
            }
        }
        if let Some((index, (old, new))) = self
            .textures
            .iter()
            .zip(&current.textures)
            .enumerate()
            .find(|(_, (a, b))| a != b)
        {
            result.push(format!("texture[{index}] {old:?} -> {new:?}"));
        }
        result
    }
}
