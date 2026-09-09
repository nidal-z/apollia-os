//! The few engine lines that answer "where did the model load", lifted out of
//! the pipe drain and re-emitted as structured runtime events.
//!
//! `llama-server` writes everything it knows to stderr, and the drain forwards
//! each line under the `llama-server` target, which the default filter
//! (`apollia=info,warn`) drops. An operator asking whether inference runs on
//! the card therefore had to relaunch the engine by hand with `-lv 5` to read
//! the answer. Measured on 2026-09-09 on Windows: thirty lines forwarded, none
//! naming a device, while the same binary launched by hand printed
//! `offloaded 41/41 layers to GPU`.
//!
//! At verbosity 4 the engine prints three kinds of line worth keeping, and
//! this module recognises them: the device it will use, the offload tally, and
//! the per-device buffer sizes. Each becomes one `apollia_runtime` event with
//! fields, so the default journal carries the answer without the noise of the
//! other two hundred lines.

/// One fact an engine line states about placement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EngineFact {
    /// `llama_prepare_model_devices: using device Vulkan0 (AMD Radeon RX 6900 XT) ...`
    Device {
        /// Backend-qualified device id, `Vulkan0`, `CUDA0`, `Metal`.
        id: String,
        /// The card's own name.
        name: String,
    },
    /// `load_tensors: offloaded 41/41 layers to GPU`
    Offload {
        /// Layers placed on the accelerator.
        offloaded: u32,
        /// Layers the model has.
        total: u32,
    },
    /// `load_tensors:      Vulkan0 model buffer size = 18802.47 MiB`, and the
    /// KV, output and compute buffers printed in the same shape.
    Buffer {
        /// Where the buffer lives, `Vulkan0`, `CPU_Mapped`, `Vulkan_Host`.
        device: String,
        /// What the buffer holds, `model`, `KV`, `compute`, `output`.
        kind: String,
        /// Size in mebibytes, as the engine prints it.
        mib: f64,
    },
}

/// Read a placement fact out of one stderr line, if the line carries one.
///
/// The engine prefixes each line with a timestamp and a level letter; the
/// match is on the message body, so the prefix's exact shape does not matter.
pub(crate) fn engine_fact(line: &str) -> Option<EngineFact> {
    if let Some(rest) = find_after(line, "using device ") {
        let (id, after_id) = rest.split_once(' ')?;
        let name = after_id.strip_prefix('(')?.split_once(')')?.0;
        return Some(EngineFact::Device {
            id: id.to_owned(),
            name: name.trim().to_owned(),
        });
    }
    if let Some(rest) = find_after(line, "offloaded ") {
        let (tally, _) = rest.split_once(' ')?;
        let (offloaded, total) = tally.split_once('/')?;
        return Some(EngineFact::Offload {
            offloaded: offloaded.parse().ok()?,
            total: total.parse().ok()?,
        });
    }
    if let Some(idx) = line.find(" buffer size = ") {
        let (head, tail) = line.split_at(idx);
        let mib: f64 = tail
            .trim_start_matches(" buffer size = ")
            .trim()
            .strip_suffix(" MiB")?
            .trim()
            .parse()
            .ok()?;
        // `... Vulkan0 model buffer size`, `... Vulkan0 KV buffer size`,
        // `... Vulkan_Host compute buffer size`: the two words before the
        // marker are the device and the kind.
        let mut words = head.split_whitespace().rev();
        let kind = words.next()?;
        let device = words.next()?;
        return Some(EngineFact::Buffer {
            device: device.to_owned(),
            kind: kind.to_owned(),
            mib,
        });
    }
    None
}

/// The text after the first occurrence of `marker`, if present.
fn find_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.find(marker).map(|i| &line[i + marker.len()..])
}

/// Emit the fact as a structured runtime event, under the runtime's own
/// target so the default filter keeps it.
pub(crate) fn emit(fact: &EngineFact) {
    match fact {
        EngineFact::Device { id, name } => {
            tracing::info!(device = %id, name = %name, "llama.server.device");
        }
        EngineFact::Offload { offloaded, total } => {
            tracing::info!(offloaded, total, "llama.server.offload");
        }
        EngineFact::Buffer { device, kind, mib } => {
            tracing::info!(device = %device, kind = %kind, mib, "llama.server.buffer");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_device_line_names_backend_id_and_card() {
        // GIVEN the device line the engine printed on 2026-09-09
        let line = "0.00.552.314 I llama_prepare_model_devices: using device Vulkan0 (AMD Radeon RX 6900 XT) (unknown id) - 15569 MiB free";

        // WHEN it is read
        let fact = engine_fact(line);

        // THEN the backend id and the card name come out, without the parenthesis
        assert_eq!(
            fact,
            Some(EngineFact::Device {
                id: "Vulkan0".to_owned(),
                name: "AMD Radeon RX 6900 XT".to_owned(),
            })
        );
    }

    #[test]
    fn the_offload_tally_reads_placed_over_total() {
        // GIVEN the tally line, full offload
        let line = "0.12.001.000 I load_tensors: offloaded 41/41 layers to GPU";

        // WHEN it is read
        let fact = engine_fact(line);

        // THEN both numbers are carried
        assert_eq!(
            fact,
            Some(EngineFact::Offload {
                offloaded: 41,
                total: 41
            })
        );
    }

    #[test]
    fn a_partial_offload_is_not_rounded_up() {
        // GIVEN a tally where the processor keeps part of the model. This is the
        // control: a reader that only knew "offloaded" would answer 41/41 here.
        let line = "0.12.001.000 I load_tensors: offloaded 20/41 layers to GPU";

        // WHEN it is read
        let fact = engine_fact(line);

        // THEN the shortfall is visible
        assert_eq!(
            fact,
            Some(EngineFact::Offload {
                offloaded: 20,
                total: 41
            })
        );
    }

    #[test]
    fn buffer_lines_carry_device_kind_and_size() {
        // GIVEN the three buffer shapes the engine prints
        let model = "0.12.0 I load_tensors:      Vulkan0 model buffer size = 18802.47 MiB";
        let kv = "0.12.0 I llama_kv_cache:    Vulkan0 KV buffer size =   640.00 MiB";
        let host = "0.12.0 I sched_reserve: Vulkan_Host compute buffer size =    40.02 MiB";

        // WHEN each is read
        // THEN device, kind and size are separated
        assert_eq!(
            engine_fact(model),
            Some(EngineFact::Buffer {
                device: "Vulkan0".to_owned(),
                kind: "model".to_owned(),
                mib: 18802.47
            })
        );
        assert_eq!(
            engine_fact(kv),
            Some(EngineFact::Buffer {
                device: "Vulkan0".to_owned(),
                kind: "KV".to_owned(),
                mib: 640.0
            })
        );
        assert_eq!(
            engine_fact(host),
            Some(EngineFact::Buffer {
                device: "Vulkan_Host".to_owned(),
                kind: "compute".to_owned(),
                mib: 40.02
            })
        );
    }

    #[test]
    fn unrelated_lines_yield_nothing() {
        // GIVEN lines the engine prints around the facts, including one that
        // mentions a device without stating placement
        let lines = [
            "0.00.1 W srv  llama_server: CORS is set to allow all origins ('*') and no API key is set",
            "0.00.1 I srv    load_model: loading model 'C:\\models\\m.gguf'",
            "0.00.1 I load_tensors: layer   0 assigned to device Vulkan0, is_swa = 0",
            "0.17.9 I slot print_timing: id  0 | task 0 | eval time = 2652.48 ms / 96 tokens",
            "",
        ];

        // WHEN each is read
        // THEN none is mistaken for a fact
        for line in lines {
            assert_eq!(engine_fact(line), None, "{line}");
        }
    }

    #[test]
    fn a_malformed_tally_is_refused_rather_than_guessed() {
        // GIVEN an offload line whose numbers are not numbers
        let line = "I load_tensors: offloaded all/41 layers to GPU";

        // WHEN it is read
        let fact = engine_fact(line);

        // THEN nothing is emitted, since a wrong count would be read as truth
        assert_eq!(fact, None);
    }
}
