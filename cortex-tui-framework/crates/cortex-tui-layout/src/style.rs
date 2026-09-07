//! Layout style types for `Cortex TUI`.
//!
//! This module provides flexbox-compatible style types that map to taffy's layout engine.

use taffy::style as taffy_style;

/// Direction for flex container main axis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FlexDirection {
    /// Items are placed in a row from left to right.
    #[default]
    Row,
    /// Items are placed in a column from top to bottom.
    Column,
    /// Items are placed in a row from right to left.
    RowReverse,
    /// Items are placed in a column from bottom to top.
    ColumnReverse,
}

impl From<FlexDirection> for taffy_style::FlexDirection {
    fn from(value: FlexDirection) -> Self {
        match value {
            FlexDirection::Row => Self::Row,
            FlexDirection::Column => Self::Column,
            FlexDirection::RowReverse => Self::RowReverse,
            FlexDirection::ColumnReverse => Self::ColumnReverse,
        }
    }
}

impl From<taffy_style::FlexDirection> for FlexDirection {
    fn from(value: taffy_style::FlexDirection) -> Self {
        match value {
            taffy_style::FlexDirection::Row => Self::Row,
            taffy_style::FlexDirection::Column => Self::Column,
            taffy_style::FlexDirection::RowReverse => Self::RowReverse,
            taffy_style::FlexDirection::ColumnReverse => Self::ColumnReverse,
        }
    }
}

/// Whether flex items wrap to multiple lines.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum FlexWrap {
    /// Items do not wrap.
    #[default]
    NoWrap,
    /// Items wrap to the next line.
    Wrap,
    /// Items wrap to the previous line.
    WrapReverse,
}

impl From<FlexWrap> for taffy_style::FlexWrap {
    fn from(value: FlexWrap) -> Self {
        match value {
            FlexWrap::NoWrap => Self::NoWrap,
            FlexWrap::Wrap => Self::Wrap,
            FlexWrap::WrapReverse => Self::WrapReverse,
        }
    }
}

impl From<taffy_style::FlexWrap> for FlexWrap {
    fn from(value: taffy_style::FlexWrap) -> Self {
        match value {
            taffy_style::FlexWrap::NoWrap => Self::NoWrap,
            // Cortex does not expose balanced wrapping; preserve line direction.
            taffy_style::FlexWrap::Wrap | taffy_style::FlexWrap::Balance => Self::Wrap,
            taffy_style::FlexWrap::WrapReverse | taffy_style::FlexWrap::BalanceReverse => {
                Self::WrapReverse
            }
        }
    }
}

/// Alignment of items along the main axis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum JustifyContent {
    /// Items are packed toward the start.
    #[default]
    Start,
    /// Items are packed toward the end.
    End,
    /// Items are centered.
    Center,
    /// Items are evenly distributed with first at start and last at end.
    SpaceBetween,
    /// Items are evenly distributed with equal space around each.
    SpaceAround,
    /// Items are evenly distributed with equal space between.
    SpaceEvenly,
}

impl From<JustifyContent> for taffy_style::JustifyContent {
    fn from(value: JustifyContent) -> Self {
        match value {
            JustifyContent::Start => Self::START,
            JustifyContent::End => Self::END,
            JustifyContent::Center => Self::CENTER,
            JustifyContent::SpaceBetween => Self::SPACE_BETWEEN,
            JustifyContent::SpaceAround => Self::SPACE_AROUND,
            JustifyContent::SpaceEvenly => Self::SPACE_EVENLY,
        }
    }
}

impl From<taffy_style::JustifyContent> for JustifyContent {
    fn from(value: taffy_style::JustifyContent) -> Self {
        // The Cortex style subset has no overflow-safety modifier.
        match value.keyword() {
            taffy_style::AlignContentKeyword::Start
            | taffy_style::AlignContentKeyword::FlexStart
            | taffy_style::AlignContentKeyword::Stretch => Self::Start,
            taffy_style::AlignContentKeyword::End | taffy_style::AlignContentKeyword::FlexEnd => {
                Self::End
            }
            taffy_style::AlignContentKeyword::Center => Self::Center,
            taffy_style::AlignContentKeyword::SpaceBetween => Self::SpaceBetween,
            taffy_style::AlignContentKeyword::SpaceAround => Self::SpaceAround,
            taffy_style::AlignContentKeyword::SpaceEvenly => Self::SpaceEvenly,
        }
    }
}

/// Alignment of items along the cross axis.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AlignItems {
    /// Items are aligned at the start of the cross axis.
    Start,
    /// Items are aligned at the end of the cross axis.
    End,
    /// Items are centered along the cross axis.
    Center,
    /// Items are aligned at their baselines.
    Baseline,
    /// Items are stretched to fill the container.
    #[default]
    Stretch,
}

impl From<AlignItems> for taffy_style::AlignItems {
    fn from(value: AlignItems) -> Self {
        match value {
            AlignItems::Start => Self::START,
            AlignItems::End => Self::END,
            AlignItems::Center => Self::CENTER,
            AlignItems::Baseline => Self::BASELINE,
            AlignItems::Stretch => Self::STRETCH,
        }
    }
}

impl From<taffy_style::AlignItems> for AlignItems {
    fn from(value: taffy_style::AlignItems) -> Self {
        // Normalize safety and direction-relative keywords to the Cortex subset.
        match value.keyword() {
            taffy_style::AlignItemsKeyword::Start
            | taffy_style::AlignItemsKeyword::FlexStart
            | taffy_style::AlignItemsKeyword::SelfStart => Self::Start,
            taffy_style::AlignItemsKeyword::End
            | taffy_style::AlignItemsKeyword::FlexEnd
            | taffy_style::AlignItemsKeyword::SelfEnd => Self::End,
            taffy_style::AlignItemsKeyword::Center => Self::Center,
            taffy_style::AlignItemsKeyword::Baseline => Self::Baseline,
            taffy_style::AlignItemsKeyword::Stretch => Self::Stretch,
        }
    }
}

/// Alignment of lines within a flex container when there is extra space.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AlignContent {
    /// Lines are packed toward the start.
    Start,
    /// Lines are packed toward the end.
    End,
    /// Lines are centered.
    Center,
    /// Lines are evenly distributed with first at start and last at end.
    SpaceBetween,
    /// Lines are evenly distributed with equal space around each.
    SpaceAround,
    /// Lines are evenly distributed with equal space between.
    SpaceEvenly,
    /// Lines are stretched to fill the container.
    #[default]
    Stretch,
}

impl From<AlignContent> for taffy_style::AlignContent {
    fn from(value: AlignContent) -> Self {
        match value {
            AlignContent::Start => Self::START,
            AlignContent::End => Self::END,
            AlignContent::Center => Self::CENTER,
            AlignContent::SpaceBetween => Self::SPACE_BETWEEN,
            AlignContent::SpaceAround => Self::SPACE_AROUND,
            AlignContent::SpaceEvenly => Self::SPACE_EVENLY,
            AlignContent::Stretch => Self::STRETCH,
        }
    }
}

impl From<taffy_style::AlignContent> for AlignContent {
    fn from(value: taffy_style::AlignContent) -> Self {
        match value.keyword() {
            taffy_style::AlignContentKeyword::Start
            | taffy_style::AlignContentKeyword::FlexStart => Self::Start,
            taffy_style::AlignContentKeyword::End | taffy_style::AlignContentKeyword::FlexEnd => {
                Self::End
            }
            taffy_style::AlignContentKeyword::Center => Self::Center,
            taffy_style::AlignContentKeyword::SpaceBetween => Self::SpaceBetween,
            taffy_style::AlignContentKeyword::SpaceAround => Self::SpaceAround,
            taffy_style::AlignContentKeyword::SpaceEvenly => Self::SpaceEvenly,
            taffy_style::AlignContentKeyword::Stretch => Self::Stretch,
        }
    }
}

/// Override for cross-axis alignment of a single flex item.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AlignSelf {
    /// Use the parent's align-items value.
    #[default]
    Auto,
    /// Align at the start of the cross axis.
    Start,
    /// Align at the end of the cross axis.
    End,
    /// Center along the cross axis.
    Center,
    /// Align at baseline.
    Baseline,
    /// Stretch to fill.
    Stretch,
}

impl AlignSelf {
    /// Converts to an `Option<taffy::AlignItems>` since taffy doesn't have `AlignSelf::Auto`.
    #[must_use]
    pub fn to_taffy_option(&self) -> Option<taffy_style::AlignItems> {
        match *self {
            AlignSelf::Auto => None,
            AlignSelf::Start => Some(taffy_style::AlignItems::START),
            AlignSelf::End => Some(taffy_style::AlignItems::END),
            AlignSelf::Center => Some(taffy_style::AlignItems::CENTER),
            AlignSelf::Baseline => Some(taffy_style::AlignItems::BASELINE),
            AlignSelf::Stretch => Some(taffy_style::AlignItems::STRETCH),
        }
    }
}

/// A dimension value that can be auto, points, or percentage.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Dimension {
    /// Automatically computed dimension.
    #[default]
    Auto,
    /// Fixed size in points (usually terminal cells).
    Points(f32),
    /// Percentage of parent dimension (0.0 to 100.0).
    Percent(f32),
}

impl Dimension {
    /// Creates a dimension from points.
    #[must_use]
    pub const fn points(value: f32) -> Self {
        Self::Points(value)
    }

    /// Creates a dimension from a percentage.
    #[must_use]
    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }

    /// Creates an auto dimension.
    #[must_use]
    pub const fn auto() -> Self {
        Self::Auto
    }

    /// Checks if this dimension is auto.
    #[must_use]
    pub const fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }

    /// Resolves the dimension to a concrete value given a parent size.
    #[must_use]
    pub fn resolve(&self, parent_size: f32) -> Option<f32> {
        match *self {
            Self::Auto => None,
            Self::Points(p) => Some(p),
            Self::Percent(pct) => Some(parent_size * pct / 100.0),
        }
    }
}

impl From<f32> for Dimension {
    fn from(value: f32) -> Self {
        Self::Points(value)
    }
}

impl From<i32> for Dimension {
    #[allow(clippy::cast_precision_loss)]
    fn from(value: i32) -> Self {
        Self::Points(value as f32)
    }
}

impl From<Dimension> for taffy_style::Dimension {
    fn from(value: Dimension) -> Self {
        taffy_style::LengthPercentageAuto::from(value).into()
    }
}

impl From<Dimension> for taffy_style::LengthPercentageAuto {
    fn from(value: Dimension) -> Self {
        match value {
            Dimension::Auto => Self::auto(),
            Dimension::Points(p) => Self::length(p),
            Dimension::Percent(pct) => Self::percent(pct / 100.0),
        }
    }
}

/// A layout value cannot be represented by the Cortex style subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnsupportedDimension;

impl std::fmt::Display for UnsupportedDimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("The layout dimension is not supported")
    }
}

impl std::error::Error for UnsupportedDimension {}

impl TryFrom<taffy_style::Dimension> for Dimension {
    type Error = UnsupportedDimension;

    fn try_from(value: taffy_style::Dimension) -> Result<Self, Self::Error> {
        use taffy_style::ExpandedDimension;
        match value.expand() {
            ExpandedDimension::Length(l) => Ok(Self::Points(l)),
            ExpandedDimension::Percent(p) => Ok(Self::Percent(p * 100.0)),
            ExpandedDimension::Auto => Ok(Self::Auto),
            ExpandedDimension::MinContent
            | ExpandedDimension::MaxContent
            | ExpandedDimension::FitContent
            | ExpandedDimension::FitContentPx(_)
            | ExpandedDimension::FitContentPercent(_)
            | ExpandedDimension::Stretch
            | ExpandedDimension::Content
            | ExpandedDimension::Calc(_) => Err(UnsupportedDimension),
        }
    }
}

/// A length or percentage value (cannot be auto).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthPercentage {
    /// Fixed size in points.
    Points(f32),
    /// Percentage of parent dimension (0.0 to 100.0).
    Percent(f32),
}

impl Default for LengthPercentage {
    fn default() -> Self {
        Self::Points(0.0)
    }
}

impl LengthPercentage {
    /// Creates from points.
    #[must_use]
    pub const fn points(value: f32) -> Self {
        Self::Points(value)
    }

    /// Creates from percentage.
    #[must_use]
    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }

    /// Creates a zero value.
    #[must_use]
    pub const fn zero() -> Self {
        Self::Points(0.0)
    }

    /// Resolves to a concrete value given a parent size.
    #[must_use]
    pub fn resolve(&self, parent_size: f32) -> f32 {
        match *self {
            Self::Points(p) => p,
            Self::Percent(pct) => parent_size * pct / 100.0,
        }
    }
}

impl From<f32> for LengthPercentage {
    fn from(value: f32) -> Self {
        Self::Points(value)
    }
}

impl From<LengthPercentage> for taffy_style::LengthPercentage {
    fn from(value: LengthPercentage) -> Self {
        match value {
            LengthPercentage::Points(p) => Self::length(p),
            LengthPercentage::Percent(pct) => Self::percent(pct / 100.0),
        }
    }
}

impl TryFrom<taffy_style::LengthPercentage> for LengthPercentage {
    type Error = UnsupportedDimension;

    fn try_from(value: taffy_style::LengthPercentage) -> Result<Self, Self::Error> {
        match value.expand() {
            taffy_style::ExpandedLengthPercentage::Length(l) => Ok(Self::Points(l)),
            taffy_style::ExpandedLengthPercentage::Percent(p) => Ok(Self::Percent(p * 100.0)),
            taffy_style::ExpandedLengthPercentage::Calc(_) => Err(UnsupportedDimension),
        }
    }
}

/// A length, percentage, or auto value.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum LengthPercentageAuto {
    /// Automatically computed.
    #[default]
    Auto,
    /// Fixed size in points.
    Points(f32),
    /// Percentage of parent dimension (0.0 to 100.0).
    Percent(f32),
}

impl LengthPercentageAuto {
    /// Creates from points.
    #[must_use]
    pub const fn points(value: f32) -> Self {
        Self::Points(value)
    }

    /// Creates from percentage.
    #[must_use]
    pub const fn percent(value: f32) -> Self {
        Self::Percent(value)
    }

    /// Creates an auto value.
    #[must_use]
    pub const fn auto() -> Self {
        Self::Auto
    }

    /// Creates a zero value.
    #[must_use]
    pub const fn zero() -> Self {
        Self::Points(0.0)
    }

    /// Resolves to a concrete value given a parent size.
    #[must_use]
    pub fn resolve(&self, parent_size: f32) -> Option<f32> {
        match *self {
            Self::Auto => None,
            Self::Points(p) => Some(p),
            Self::Percent(pct) => Some(parent_size * pct / 100.0),
        }
    }
}

impl From<f32> for LengthPercentageAuto {
    fn from(value: f32) -> Self {
        Self::Points(value)
    }
}

impl From<LengthPercentageAuto> for taffy_style::LengthPercentageAuto {
    fn from(value: LengthPercentageAuto) -> Self {
        match value {
            LengthPercentageAuto::Auto => Self::auto(),
            LengthPercentageAuto::Points(p) => Self::length(p),
            LengthPercentageAuto::Percent(pct) => Self::percent(pct / 100.0),
        }
    }
}

impl TryFrom<taffy_style::LengthPercentageAuto> for LengthPercentageAuto {
    type Error = UnsupportedDimension;

    fn try_from(value: taffy_style::LengthPercentageAuto) -> Result<Self, Self::Error> {
        match value.expand() {
            taffy_style::ExpandedLengthPercentageAuto::Auto => Ok(Self::Auto),
            taffy_style::ExpandedLengthPercentageAuto::Length(l) => Ok(Self::Points(l)),
            taffy_style::ExpandedLengthPercentageAuto::Percent(p) => Ok(Self::Percent(p * 100.0)),
            taffy_style::ExpandedLengthPercentageAuto::Calc(_) => Err(UnsupportedDimension),
        }
    }
}

/// Edge values for padding, margin, and border.
///
/// Represents values for top, right, bottom, and left edges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Edges<T> {
    /// Top edge value.
    pub top: T,
    /// Right edge value.
    pub right: T,
    /// Bottom edge value.
    pub bottom: T,
    /// Left edge value.
    pub left: T,
}

impl<T: Default> Default for Edges<T> {
    fn default() -> Self {
        Self {
            top: T::default(),
            right: T::default(),
            bottom: T::default(),
            left: T::default(),
        }
    }
}

impl<T: Clone> Edges<T> {
    /// Creates edges with the same value on all sides.
    pub fn all(value: T) -> Self {
        Self {
            top: value.clone(),
            right: value.clone(),
            bottom: value.clone(),
            left: value,
        }
    }

    /// Creates edges with separate horizontal and vertical values.
    pub fn axes(vertical: T, horizontal: T) -> Self {
        Self {
            top: vertical.clone(),
            right: horizontal.clone(),
            bottom: vertical,
            left: horizontal,
        }
    }

    /// Creates edges with a single value for each side.
    pub fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Maps each value in the edges using a function.
    pub fn map<U, F: Fn(T) -> U>(self, f: F) -> Edges<U> {
        Edges {
            top: f(self.top),
            right: f(self.right),
            bottom: f(self.bottom),
            left: f(self.left),
        }
    }
}

impl<T: Copy + Default> Edges<T> {
    /// Creates edges with zero values.
    pub fn zero() -> Self
    where
        T: From<f32>,
    {
        Self::all(T::from(0.0))
    }
}

impl From<Edges<LengthPercentage>> for taffy::Rect<taffy_style::LengthPercentage> {
    fn from(value: Edges<LengthPercentage>) -> Self {
        Self {
            top: value.top.into(),
            right: value.right.into(),
            bottom: value.bottom.into(),
            left: value.left.into(),
        }
    }
}

impl From<Edges<LengthPercentageAuto>> for taffy::Rect<taffy_style::LengthPercentageAuto> {
    fn from(value: Edges<LengthPercentageAuto>) -> Self {
        Self {
            top: value.top.into(),
            right: value.right.into(),
            bottom: value.bottom.into(),
            left: value.left.into(),
        }
    }
}

/// Position type for layout nodes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Position {
    /// Element participates in normal layout flow.
    #[default]
    Relative,
    /// Element is positioned relative to its normal position.
    Absolute,
}

impl From<Position> for taffy_style::Position {
    fn from(value: Position) -> Self {
        match value {
            Position::Relative => Self::Relative,
            Position::Absolute => Self::Absolute,
        }
    }
}

impl From<taffy_style::Position> for Position {
    fn from(value: taffy_style::Position) -> Self {
        match value {
            taffy_style::Position::Relative => Self::Relative,
            taffy_style::Position::Absolute => Self::Absolute,
        }
    }
}

/// Overflow behavior for content that exceeds container bounds.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Overflow {
    /// Content is not clipped and may render outside the container.
    #[default]
    Visible,
    /// Content is clipped to the container bounds.
    Hidden,
    /// Content is clipped but scrollable.
    Scroll,
}

impl From<Overflow> for taffy_style::Overflow {
    fn from(value: Overflow) -> Self {
        match value {
            Overflow::Visible => Self::Visible,
            Overflow::Hidden => Self::Hidden,
            Overflow::Scroll => Self::Scroll,
        }
    }
}

impl From<taffy_style::Overflow> for Overflow {
    fn from(value: taffy_style::Overflow) -> Self {
        match value {
            taffy_style::Overflow::Visible => Self::Visible,
            taffy_style::Overflow::Hidden => Self::Hidden,
            taffy_style::Overflow::Scroll => Self::Scroll,
            taffy_style::Overflow::Clip => Self::Hidden,
        }
    }
}

/// Display mode for layout nodes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Display {
    /// Normal flex display.
    #[default]
    Flex,
    /// Node is hidden and does not participate in layout.
    None,
}

impl From<Display> for taffy_style::Display {
    fn from(value: Display) -> Self {
        match value {
            Display::Flex => Self::Flex,
            Display::None => Self::None,
        }
    }
}

impl From<taffy_style::Display> for Display {
    fn from(value: taffy_style::Display) -> Self {
        match value {
            taffy_style::Display::None => Self::None,
            _ => Self::Flex,
        }
    }
}

/// Size type with width and height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size<T> {
    /// Width dimension.
    pub width: T,
    /// Height dimension.
    pub height: T,
}

impl<T: Default> Default for Size<T> {
    fn default() -> Self {
        Self {
            width: T::default(),
            height: T::default(),
        }
    }
}

impl<T: Clone> Size<T> {
    /// Creates a new size.
    pub fn new(width: T, height: T) -> Self {
        Self { width, height }
    }

    /// Creates a size with the same value for width and height.
    pub fn square(value: T) -> Self {
        Self {
            width: value.clone(),
            height: value,
        }
    }
}

impl<T> Size<T>
where
    T: From<f32>,
{
    /// Creates a zero size.
    pub fn zero() -> Self {
        Self {
            width: T::from(0.0),
            height: T::from(0.0),
        }
    }
}

impl Size<Dimension> {
    /// Creates an auto-sized size.
    pub fn auto() -> Self {
        Self {
            width: Dimension::Auto,
            height: Dimension::Auto,
        }
    }
}

impl From<Size<Dimension>> for taffy::Size<taffy_style::Dimension> {
    fn from(value: Size<Dimension>) -> Self {
        Self {
            width: value.width.into(),
            height: value.height.into(),
        }
    }
}

impl From<Size<Dimension>> for taffy::Size<taffy_style::LengthPercentageAuto> {
    fn from(value: Size<Dimension>) -> Self {
        Self {
            width: value.width.into(),
            height: value.height.into(),
        }
    }
}

impl From<Size<LengthPercentage>> for taffy::Size<taffy_style::LengthPercentage> {
    fn from(value: Size<LengthPercentage>) -> Self {
        Self {
            width: value.width.into(),
            height: value.height.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimension_resolve() {
        assert_eq!(Dimension::Points(10.0).resolve(100.0), Some(10.0));
        assert_eq!(Dimension::Percent(50.0).resolve(100.0), Some(50.0));
        assert_eq!(Dimension::Auto.resolve(100.0), None);
    }

    #[test]
    fn test_edges_all() {
        let edges = Edges::all(LengthPercentage::Points(5.0));
        assert_eq!(edges.top, LengthPercentage::Points(5.0));
        assert_eq!(edges.right, LengthPercentage::Points(5.0));
        assert_eq!(edges.bottom, LengthPercentage::Points(5.0));
        assert_eq!(edges.left, LengthPercentage::Points(5.0));
    }

    #[test]
    fn test_edges_axes() {
        let edges = Edges::axes(
            LengthPercentage::Points(10.0),
            LengthPercentage::Points(5.0),
        );
        assert_eq!(edges.top, LengthPercentage::Points(10.0));
        assert_eq!(edges.bottom, LengthPercentage::Points(10.0));
        assert_eq!(edges.left, LengthPercentage::Points(5.0));
        assert_eq!(edges.right, LengthPercentage::Points(5.0));
    }

    #[test]
    fn test_flex_direction_conversion() {
        let dir = FlexDirection::Column;
        let taffy_dir: taffy_style::FlexDirection = dir.into();
        let back: FlexDirection = taffy_dir.into();
        assert_eq!(dir, back);
    }

    #[test]
    fn test_size_auto() {
        let size = Size::<Dimension>::auto();
        assert!(size.width.is_auto());
        assert!(size.height.is_auto());
    }
}
