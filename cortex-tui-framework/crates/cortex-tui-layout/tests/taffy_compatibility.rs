use cortex_tui_buffer::{Buffer, Rect, Style};
use cortex_tui_layout::{
    AlignContent, AlignItems, AlignSelf, Dimension, Edges, FlexWrap, JustifyContent, LayoutStyle,
    LayoutTree, LengthPercentage, LengthPercentageAuto, StyleSize as Size,
};
use taffy::style as ts;

#[test]
fn test_dimension_conversions_preserve_units_and_auto() {
    for (value, expected) in [
        (Dimension::Auto, ts::LengthPercentageAuto::auto()),
        (
            Dimension::Points(12.5),
            ts::LengthPercentageAuto::length(12.5),
        ),
        (
            Dimension::Percent(25.0),
            ts::LengthPercentageAuto::percent(0.25),
        ),
    ] {
        let actual: ts::Dimension = value.into();
        assert_eq!(actual, ts::Dimension::from(expected));
        assert_eq!(Dimension::try_from(actual).unwrap(), value);
        assert_eq!(ts::LengthPercentageAuto::from(value), expected);
    }
}

#[test]
fn test_spacing_conversions_preserve_units_and_auto() {
    for (value, expected) in [
        (
            LengthPercentage::Points(4.0),
            ts::LengthPercentage::length(4.0),
        ),
        (
            LengthPercentage::Percent(50.0),
            ts::LengthPercentage::percent(0.5),
        ),
    ] {
        assert_eq!(ts::LengthPercentage::from(value), expected);
        assert_eq!(LengthPercentage::try_from(expected).unwrap(), value);
    }
    for (value, expected) in [
        (LengthPercentageAuto::Auto, ts::LengthPercentageAuto::auto()),
        (
            LengthPercentageAuto::Points(-2.0),
            ts::LengthPercentageAuto::length(-2.0),
        ),
        (
            LengthPercentageAuto::Percent(50.0),
            ts::LengthPercentageAuto::percent(0.5),
        ),
    ] {
        assert_eq!(ts::LengthPercentageAuto::from(value), expected);
        assert_eq!(LengthPercentageAuto::try_from(expected).unwrap(), value);
    }
}

#[test]
fn test_alignment_round_trips() {
    for value in [
        JustifyContent::Start,
        JustifyContent::End,
        JustifyContent::Center,
        JustifyContent::SpaceBetween,
        JustifyContent::SpaceAround,
        JustifyContent::SpaceEvenly,
    ] {
        let converted = ts::JustifyContent::from(value);
        assert_eq!(converted.safety, ts::AlignmentSafety::Unsafe);
        assert_eq!(JustifyContent::from(converted), value);
    }
    for value in [
        AlignContent::Start,
        AlignContent::End,
        AlignContent::Center,
        AlignContent::Stretch,
        AlignContent::SpaceBetween,
        AlignContent::SpaceAround,
        AlignContent::SpaceEvenly,
    ] {
        assert_eq!(AlignContent::from(ts::AlignContent::from(value)), value);
    }
    for (value, align_self) in [
        (AlignItems::Start, AlignSelf::Start),
        (AlignItems::End, AlignSelf::End),
        (AlignItems::Center, AlignSelf::Center),
        (AlignItems::Baseline, AlignSelf::Baseline),
        (AlignItems::Stretch, AlignSelf::Stretch),
    ] {
        let converted = ts::AlignItems::from(value);
        assert_eq!(AlignItems::from(converted), value);
        assert_eq!(align_self.to_taffy_option(), Some(converted));
    }
    assert_eq!(AlignSelf::Auto.to_taffy_option(), None);
}

#[test]
fn test_unsupported_dimensions_fail_conversion() {
    let calc_handle = std::ptr::NonNull::<u64>::dangling().as_ptr().cast();
    for value in [
        ts::Dimension::min_content(),
        ts::Dimension::max_content(),
        ts::Dimension::fit_content(),
        ts::Dimension::fit_content_px(20.0),
        ts::Dimension::fit_content_percent(0.5),
        ts::Dimension::stretch(),
        ts::Dimension::content(),
        ts::Dimension::calc(calc_handle),
    ] {
        assert!(Dimension::try_from(value).is_err());
    }
    assert!(LengthPercentage::try_from(ts::LengthPercentage::calc(calc_handle)).is_err());
    assert!(LengthPercentageAuto::try_from(ts::LengthPercentageAuto::calc(calc_handle)).is_err());
}

#[test]
fn test_new_taffy_alignment_and_wrapping_normalize_to_supported_subset() {
    assert_eq!(
        AlignItems::from(ts::AlignItems::SAFE_SELF_START),
        AlignItems::Start
    );
    assert_eq!(
        AlignItems::from(ts::AlignItems::SAFE_SELF_END),
        AlignItems::End
    );
    assert_eq!(
        JustifyContent::from(ts::JustifyContent::SAFE_CENTER),
        JustifyContent::Center
    );
    assert_eq!(
        AlignContent::from(ts::AlignContent::SAFE_FLEX_END),
        AlignContent::End
    );
    assert_eq!(FlexWrap::from(ts::FlexWrap::Balance), FlexWrap::Wrap);
    assert_eq!(
        FlexWrap::from(ts::FlexWrap::BalanceReverse),
        FlexWrap::WrapReverse
    );
}

#[test]
fn test_size_constraints_use_length_percentage_auto() {
    let style = LayoutStyle {
        min_size: Size::new(Dimension::Points(10.0), Dimension::Percent(25.0)),
        max_size: Size::new(Dimension::Percent(75.0), Dimension::Auto),
        ..Default::default()
    }
    .to_taffy();
    assert_eq!(style.min_size.width, ts::LengthPercentageAuto::length(10.0));
    assert_eq!(
        style.min_size.height,
        ts::LengthPercentageAuto::percent(0.25)
    );
    assert_eq!(
        style.max_size.width,
        ts::LengthPercentageAuto::percent(0.75)
    );
    assert_eq!(style.max_size.height, ts::LengthPercentageAuto::auto());
}

#[test]
fn test_headless_constrained_layout_at_narrow_and_wide_sizes() {
    let mut tree = LayoutTree::new();
    let root = tree
        .create_node(LayoutStyle {
            size: Size::new(Dimension::Percent(100.0), Dimension::Percent(100.0)),
            padding: Edges::all(LengthPercentage::Points(1.0)),
            align_items: AlignItems::Start,
            ..Default::default()
        })
        .unwrap();
    let child = tree
        .create_node(LayoutStyle {
            size: Size::new(Dimension::Percent(50.0), Dimension::Points(2.0)),
            min_size: Size::new(Dimension::Points(24.0), Dimension::Auto),
            max_size: Size::new(Dimension::Points(48.0), Dimension::Auto),
            margin: Edges::all(LengthPercentageAuto::Points(0.0)),
            ..Default::default()
        })
        .unwrap();
    tree.add_child(root, child).unwrap();
    tree.set_root(root).unwrap();

    for (width, height, expected_width) in [(40u16, 12u16, 24u16), (120, 40, 48), (40, 12, 24)] {
        tree.compute_layout(f32::from(width), f32::from(height));
        let layout = tree.get_computed_layout(child).unwrap();
        assert_eq!(
            (layout.x, layout.y, layout.width, layout.height),
            (1.0, 1.0, f32::from(expected_width), 2.0)
        );
        let mut buffer = Buffer::new(width, height);
        buffer.fill(
            Rect::new(
                layout.x as i32,
                layout.y as i32,
                layout.width as u16,
                layout.height as u16,
            ),
            '#',
            Style::default(),
        );
        let snapshot: Vec<String> = (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer.get(x, y).unwrap().character)
                    .collect()
            })
            .collect();
        let empty_row = " ".repeat(usize::from(width));
        let filled_row = format!(
            " {}{}",
            "#".repeat(usize::from(expected_width)),
            " ".repeat(usize::from(width - expected_width - 1))
        );
        let mut expected = vec![empty_row; usize::from(height)];
        expected[1] = filled_row.clone();
        expected[2] = filled_row;
        assert_eq!(snapshot, expected);
    }
}
