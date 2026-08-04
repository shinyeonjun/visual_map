use std::collections::BTreeMap;

use super::model::{
    properties, CollectedEvidence, CollectedFact, CollectedRelation, CollectionMode,
    CollectionStatus, CollectorResult, TruthClass,
};

const ID: &str = "messaging";

struct Signal {
    marker: &'static str,
    technology: &'static str,
    action: &'static str,
}

const SIGNALS: &[Signal] = &[
    Signal {
        marker: "@KafkaListener",
        technology: "kafka",
        action: "consume",
    },
    Signal {
        marker: "@RabbitListener",
        technology: "rabbitmq",
        action: "consume",
    },
    Signal {
        marker: "@SqsListener",
        technology: "aws-sqs",
        action: "consume",
    },
    Signal {
        marker: "@EventPattern",
        technology: "nestjs",
        action: "consume",
    },
    Signal {
        marker: "@MessagePattern",
        technology: "nestjs",
        action: "consume",
    },
    Signal {
        marker: "ServiceBusTrigger(",
        technology: "azure-service-bus",
        action: "consume",
    },
    Signal {
        marker: "KafkaTrigger(",
        technology: "kafka",
        action: "consume",
    },
    Signal {
        marker: "kafkaTemplate.send(",
        technology: "kafka",
        action: "publish",
    },
    Signal {
        marker: ".basic_publish(",
        technology: "rabbitmq",
        action: "publish",
    },
    Signal {
        marker: "new ProducerRecord<>(",
        technology: "kafka",
        action: "publish",
    },
];

pub(crate) fn collect(snapshot: &crate::SourceSnapshot) -> CollectorResult {
    let mut result = CollectorResult::new(ID, "message-flow", CollectionMode::Passive);
    let root_key = "messaging:root";
    for (path, source) in &snapshot.files {
        let Some(language) = crate::LANGUAGES
            .iter()
            .find(|language| crate::frameworks::path_matches_language(path, language.extensions))
        else {
            continue;
        };
        let code = crate::frameworks::source_code_mask(source, language.id);
        for (line_index, (line, code_line)) in source.lines().zip(code.lines()).enumerate() {
            for signal in SIGNALS {
                let Some(mask_offset) = code_line.find(signal.marker) else {
                    continue;
                };
                let character_offset = code_line[..mask_offset].chars().count();
                let offset = line
                    .char_indices()
                    .nth(character_offset)
                    .map(|(offset, _)| offset)
                    .unwrap_or(line.len());
                let argument = &line[offset + signal.marker.len()..];
                let channel = first_string_literal(argument);
                let key = format!(
                    "message-endpoint:{}:{}:{}:{}",
                    signal.action,
                    signal.technology,
                    path,
                    line_index + 1
                );
                result.facts.push(CollectedFact {
                    stable_key: key.clone(),
                    kind: match signal.action {
                        "consume" => "message-consumer",
                        _ => "message-producer",
                    }
                    .to_string(),
                    name: channel.unwrap_or(signal.marker).to_string(),
                    path: Some(path.clone()),
                    properties: properties(&[
                        ("technology", Some(signal.technology)),
                        ("action", Some(signal.action)),
                        ("channel", channel),
                        (
                            "resolution",
                            Some(if channel.is_some() {
                                "static"
                            } else {
                                "dynamic"
                            }),
                        ),
                    ]),
                });
                result.relations.push(CollectedRelation {
                    from: root_key.to_string(),
                    to: key,
                    kind: "CONTAINS".to_string(),
                    truth_class: TruthClass::Confirmed,
                    evidence_type: "MESSAGE_API".to_string(),
                    evidence: vec![CollectedEvidence {
                        path: path.clone(),
                        line: Some((line_index + 1) as u32),
                        note: Some(signal.marker.to_string()),
                    }],
                    properties: BTreeMap::new(),
                });
                result.summary.detected_by.push(path.clone());
            }
        }
    }
    if result.facts.is_empty() {
        return result;
    }
    result.facts.push(CollectedFact {
        stable_key: root_key.to_string(),
        kind: "messaging-topology".to_string(),
        name: "Messaging".to_string(),
        path: None,
        properties: BTreeMap::new(),
    });
    result.summary.detected_by.sort();
    result.summary.detected_by.dedup();
    result.summary.status = CollectionStatus::Collected;
    result
}

fn first_string_literal(value: &str) -> Option<&str> {
    let (start, quote) = value
        .char_indices()
        .find(|(_, character)| matches!(character, '\'' | '"'))?;
    let rest = &value[start + quote.len_utf8()..];
    let end = rest.find(quote)?;
    (!rest[..end].is_empty()).then_some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::collect;

    #[test]
    fn extracts_only_explicit_message_apis() {
        let snapshot = crate::SourceSnapshot {
            files: vec![(
                "src/Orders.java".to_string(),
                "// @KafkaListener(topics = \"ignored\")\n@KafkaListener(topics = \"orders\")\nvoid consume() {}\nkafkaTemplate.send(\"receipts\", value);\n"
                    .to_string(),
            )],
            file_hashes: Default::default(),
            source_paths: Vec::new(),
        };

        let result = collect(&snapshot);
        assert_eq!(
            result
                .facts
                .iter()
                .filter(|fact| fact.kind == "message-consumer")
                .count(),
            1
        );
        assert!(result
            .facts
            .iter()
            .any(|fact| fact.kind == "message-producer" && fact.name == "receipts"));
    }
}
