use fs80h_core::{
    FingerPosition, FingerView, ImpressionType, Template, TemplateFormat, TemplateMinutia,
    TemplateMinutiaKind,
};

fn template() -> Template {
    Template {
        product_owner: 0,
        product_type: 0,
        device_id: 0,
        fingers: vec![FingerView {
            position: FingerPosition::RightIndex,
            view: 0,
            impression: ImpressionType::LivePlain,
            quality: 80,
            width: 320,
            height: 480,
            x_resolution: 500,
            y_resolution: 500,
            minutiae: vec![TemplateMinutia {
                x: 100,
                y: 200,
                angle: 45,
                quality: 75,
                kind: TemplateMinutiaKind::RidgeEnding,
            }],
            singular_points: vec![],
        }],
    }
}

#[test]
fn template_round_trips_all_supported_profiles() {
    for format in [
        TemplateFormat::Iso19794_2_2005,
        TemplateFormat::Iso19794_2_2011,
        TemplateFormat::Ansi378_2004,
        TemplateFormat::Ansi378_2009,
        TemplateFormat::Ansi378_2009Am1,
    ] {
        let original = template();
        let encoded = original.to_bytes(format).unwrap();
        assert_eq!(Template::parse(format, &encoded).unwrap(), original);
        assert_eq!(
            Template::from_base64(format, &original.to_base64(format).unwrap()).unwrap(),
            original
        );
    }
}

#[test]
fn legacy_profile_rejects_amendment_multi_finger_position() {
    let mut record = template();
    record.fingers[0].position = FingerPosition::BothIndexes;
    assert!(record.to_bytes(TemplateFormat::Iso19794_2_2005).is_err());
    assert!(record.to_bytes(TemplateFormat::Ansi378_2009Am1).is_ok());
}

#[test]
fn parser_rejects_corrupted_length() {
    let mut bytes = template()
        .to_bytes(TemplateFormat::Iso19794_2_2011)
        .unwrap();
    bytes[11] = bytes[11].saturating_add(1);
    assert!(Template::parse(TemplateFormat::Iso19794_2_2011, &bytes).is_err());
}
