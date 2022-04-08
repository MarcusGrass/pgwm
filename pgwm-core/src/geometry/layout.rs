use crate::config::WS_WINDOW_LIMIT;
use crate::geometry::Dimensions;
use crate::{error::Result, push_heapless};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "config-file", derive(serde::Deserialize))]
pub enum Layout {
    LeftLeader = 0,
    CenterLeader = 1,
}

impl Layout {
    #[must_use]
    pub fn next(&self) -> Self {
        match self {
            Layout::LeftLeader => Layout::CenterLeader,
            Layout::CenterLeader => Layout::LeftLeader,
        }
    }

    #[allow(clippy::needless_range_loop)]
    #[allow(clippy::too_many_lines)]
    pub fn calculate_dimensions(
        &self,
        monitor_width: u32,
        monitor_height: u32,
        pad_len: i16,
        border_width: u32,
        status_bar_height: i16,
        pad_on_single: bool,
        num_windows: usize,
        size_modifiers: &[f32],
        left_leader_base_modifier: f32,
        center_leader_base_modifier: f32,
    ) -> Result<heapless::CopyVec<Dimensions, WS_WINDOW_LIMIT>> {
        let default_y_offset = status_bar_height;
        let border_len = border_width as i16;
        let monitor_height = monitor_height as i16 - status_bar_height;
        let monitor_width = monitor_width as i16;
        match self {
            Layout::LeftLeader => calculate_normal_dimensions(
                monitor_width,
                monitor_height,
                pad_len,
                border_len,
                pad_on_single,
                default_y_offset,
                num_windows,
                size_modifiers,
                left_leader_base_modifier,
            ),
            Layout::CenterLeader => {
                if num_windows < 3 {
                    calculate_normal_dimensions(
                        monitor_width,
                        monitor_height,
                        pad_len,
                        border_len,
                        pad_on_single,
                        default_y_offset,
                        num_windows,
                        size_modifiers,
                        left_leader_base_modifier,
                    )
                } else {
                    let mut dims = heapless::CopyVec::new();

                    // Center modifier is the only one that can be expanded, and is expanded horizontally
                    let horisontal_win_modifiers: heapless::CopyVec<f32, 3> =
                        heapless::CopyVec::from_slice(&[1.0, center_leader_base_modifier, 1.0])
                            .map_err(|_| crate::error::Error::HeaplessInstantiate)?;
                    let horisontal_x_offset_and_widths = calculate_offset_and_lengths(
                        monitor_width,
                        pad_len,
                        border_len,
                        horisontal_win_modifiers,
                    )?;
                    let mut left_aligned_modifiers: heapless::CopyVec<
                        f32,
                        { WS_WINDOW_LIMIT / 2 },
                    > = heapless::CopyVec::new();
                    let mut right_aligned_modifiers: heapless::CopyVec<
                        f32,
                        { WS_WINDOW_LIMIT / 2 },
                    > = heapless::CopyVec::new();
                    for i in 1..num_windows {
                        if i % 2 == 0 {
                            push_heapless!(left_aligned_modifiers, size_modifiers[i])?;
                        } else {
                            push_heapless!(right_aligned_modifiers, size_modifiers[i])?;
                        }
                    }
                    let left_vertical_offset_and_lengths = calculate_offset_and_lengths(
                        monitor_height,
                        pad_len,
                        border_len,
                        left_aligned_modifiers,
                    )?;
                    let right_vertical_offset_and_lengths = calculate_offset_and_lengths(
                        monitor_height,
                        pad_len,
                        border_len,
                        right_aligned_modifiers,
                    )?;
                    let master_y =
                        calculate_same_length_window_offset(0, monitor_height, pad_len, border_len)
                            + default_y_offset;
                    let master_height =
                        calculate_same_length_window_len(1, monitor_height, pad_len, border_len);
                    push_heapless!(
                        dims,
                        Dimensions::new(
                            horisontal_x_offset_and_widths[1].1,
                            master_height,
                            horisontal_x_offset_and_widths[1].0,
                            master_y
                        )
                    )?;
                    for i in 1..num_windows {
                        if i % 2 == 0 {
                            let ind = i / 2 - 1;
                            push_heapless!(
                                dims,
                                Dimensions::new(
                                    horisontal_x_offset_and_widths[0].1,
                                    left_vertical_offset_and_lengths[ind].1,
                                    horisontal_x_offset_and_widths[0].0,
                                    left_vertical_offset_and_lengths[ind].0 + default_y_offset,
                                )
                            )?;
                        } else {
                            let ind = i / 2; // We're using built in integer flooring to get this right
                            push_heapless!(
                                dims,
                                Dimensions::new(
                                    horisontal_x_offset_and_widths[2].1,
                                    right_vertical_offset_and_lengths[ind].1,
                                    horisontal_x_offset_and_widths[2].0,
                                    right_vertical_offset_and_lengths[ind].0 + default_y_offset,
                                )
                            )?;
                        }
                    }
                    Ok(dims)
                }
            }
        }
    }
}

#[allow(clippy::needless_range_loop)]
fn calculate_normal_dimensions(
    monitor_width: i16,
    monitor_height: i16,
    pad_len: i16,
    border_len: i16,
    pad_on_single: bool,
    default_y_offset: i16,
    num_windows: usize,
    size_modifiers: &[f32],
    left_leader_base_modifier: f32,
) -> Result<heapless::CopyVec<Dimensions, WS_WINDOW_LIMIT>> {
    let mut dims = heapless::CopyVec::new();
    if num_windows == 1 {
        push_heapless!(
            dims,
            calculate_single_window(
                monitor_width,
                monitor_height,
                pad_len,
                border_len,
                default_y_offset,
                pad_on_single,
            )
        )?;
    } else {
        let horizontal_win_modifiers: heapless::CopyVec<f32, 2> =
            heapless::CopyVec::from_slice(&[left_leader_base_modifier, 1.0])
                .map_err(|_| crate::error::Error::HeaplessInstantiate)?;
        let horisontal_offset_and_lengths = calculate_offset_and_lengths(
            monitor_width,
            pad_len,
            border_len,
            horizontal_win_modifiers,
        )?;
        let mut right_side_win_modifiers: heapless::CopyVec<f32, WS_WINDOW_LIMIT> =
            heapless::CopyVec::new();
        for i in 1..num_windows {
            push_heapless!(right_side_win_modifiers, size_modifiers[i])?;
        }
        let vertical_offset_and_lengths = calculate_offset_and_lengths(
            monitor_height,
            pad_len,
            border_len,
            right_side_win_modifiers,
        )?;
        let master_win_height =
            calculate_same_length_window_len(1, monitor_height, pad_len, border_len);
        let master_win_y =
            calculate_same_length_window_offset(0, master_win_height, pad_len, border_len)
                + default_y_offset;
        push_heapless!(
            dims,
            Dimensions::new(
                horisontal_offset_and_lengths[0].1,
                master_win_height,
                horisontal_offset_and_lengths[0].0,
                master_win_y
            )
        )?;
        for (y, height) in vertical_offset_and_lengths {
            push_heapless!(
                dims,
                Dimensions::new(
                    horisontal_offset_and_lengths[1].1,
                    height,
                    horisontal_offset_and_lengths[1].0,
                    y + default_y_offset
                )
            )?;
        }
    }
    Ok(dims)
}

fn calculate_single_window(
    width: i16,
    height: i16,
    pad_len: i16,
    border_len: i16,
    status_bar_height: i16,
    pad_on_single: bool,
) -> Dimensions {
    let width = if pad_on_single {
        calculate_same_length_window_len(1, width, pad_len, border_len)
    } else {
        calculate_same_length_window_len(1, width, 0, 0)
    };
    let height = if pad_on_single {
        calculate_same_length_window_len(1, height, pad_len, border_len)
    } else {
        calculate_same_length_window_len(1, height, 0, 0)
    };
    let x = if pad_on_single {
        calculate_same_length_window_offset(0, width, pad_len, border_len)
    } else {
        0
    };
    let y = if pad_on_single {
        status_bar_height + calculate_same_length_window_offset(0, height, pad_len, border_len)
    } else {
        status_bar_height
    };
    Dimensions {
        width,
        height,
        x,
        y,
    }
}

fn calculate_same_length_window_len(
    num_windows: i16,
    total_width: i16,
    pad_len: i16,
    border_len: i16,
) -> i16 {
    ((total_width - 2 * (pad_len + border_len) - (num_windows - 1) * (2 * border_len + pad_len))
        as f32
        / num_windows as f32) as i16
}

fn calculate_same_length_window_offset(
    window_order: i16,
    window_len: i16,
    pad_len: i16,
    border_len: i16,
) -> i16 {
    pad_len + window_order * (pad_len + window_len + 2 * border_len)
}

fn calculate_offset_and_lengths<const N: usize>(
    total_space: i16,
    pad_len: i16,
    border_len: i16,
    size_modifiers: heapless::CopyVec<f32, N>,
) -> Result<heapless::CopyVec<(i16, i16), N>> {
    let available_space = calculate_available_space(
        total_space,
        size_modifiers.len() as i16,
        pad_len,
        border_len,
    );
    let sum_modifiers: f32 = size_modifiers.iter().sum();
    let fit_modifier = 1f32 / sum_modifiers;
    let mut window_widths: heapless::CopyVec<i16, N> = size_modifiers
        .into_iter()
        .map(|modifier| (modifier * fit_modifier * available_space as f32) as i16)
        .collect();
    let sum_lengths: i16 = window_widths.iter().sum();
    for i in 0..(available_space - sum_lengths) as usize {
        window_widths[i] += 1;
    }
    let mut offset_and_lengths = heapless::CopyVec::new();
    let mut prev_placed_window_lengths = 0;
    for (i, width) in window_widths.into_iter().enumerate() {
        let offset =
            calculate_line_offset(i as i16, pad_len, border_len, prev_placed_window_lengths);
        push_heapless!(offset_and_lengths, (offset, width))?;
        prev_placed_window_lengths += width;
    }
    Ok(offset_and_lengths)
}

fn calculate_line_offset(
    window_order: i16,
    pad_len: i16,
    border_len: i16,
    previously_placed_window_lengths: i16,
) -> i16 {
    (window_order + 1) * pad_len + window_order * 2 * border_len + previously_placed_window_lengths
}

fn calculate_available_space(
    total_space: i16,
    num_windows: i16,
    pad_len: i16,
    border_len: i16,
) -> i16 {
    total_space - ((num_windows + 1) * pad_len + 2 * num_windows * border_len)
}

#[cfg(test)]
mod tests {
    use crate::config::WS_WINDOW_LIMIT;
    use crate::geometry::layout::Layout;
    use crate::geometry::Dimensions;
    const TEST_WIDTH: u32 = 1000;
    const TEST_HEIGHT: u32 = 1000;
    const TEST_PAD: i16 = 5;
    const TEST_BORDER: u32 = 10;
    const TEST_STATUS_HEIGHT: i16 = 20;

    #[test]
    fn test_calculate_normal_single() {
        let tiling_dims = calculate_dimensions(1, true);
        assert_eq!(1, tiling_dims.len());
        let single = tiling_dims[0];
        assert_eq!(
            TEST_WIDTH as i16 - 2 * TEST_PAD - 2 * TEST_BORDER as i16,
            single.width
        );
        assert_eq!(
            TEST_HEIGHT as i16 - 2 * TEST_PAD - 2 * TEST_BORDER as i16 - TEST_STATUS_HEIGHT,
            single.height
        );
        assert_eq!(TEST_PAD, single.x);
        assert_eq!(TEST_PAD + TEST_STATUS_HEIGHT, single.y);
    }

    #[test]
    fn test_calculate_normal_single_no_pad() {
        let tiling_dims = calculate_dimensions(1, false);
        assert_eq!(1, tiling_dims.len());
        let single = tiling_dims[0];
        assert_eq!(TEST_WIDTH as i16, single.width);
        assert_eq!(TEST_HEIGHT as i16 - TEST_STATUS_HEIGHT, single.height);
        assert_eq!(0, single.x);
        assert_eq!(TEST_STATUS_HEIGHT, single.y);
    }

    #[test]
    fn test_leader_left_two_windows() {
        let tiling_dims = calculate_dimensions(2, false);
        assert_eq!(2, tiling_dims.len());
        let expected_height =
            TEST_HEIGHT as i16 - TEST_STATUS_HEIGHT - 2 * TEST_PAD - 2 * TEST_BORDER as i16;
        // Height should be the same
        assert_eq!(expected_height, tiling_dims[0].height);
        assert_eq!(expected_height, tiling_dims[1].height);
        // Y offset as well
        let expected_y = TEST_STATUS_HEIGHT + TEST_PAD;
        assert_eq!(expected_y, tiling_dims[0].y);
        assert_eq!(expected_y, tiling_dims[1].y);
        let right_x = tiling_dims[0].x + tiling_dims[0].width + 2 * TEST_BORDER as i16 + TEST_PAD;
        assert_eq!(right_x, tiling_dims[1].x);
        assert_eq!(
            TEST_WIDTH as i16,
            right_x + TEST_BORDER as i16 + tiling_dims[1].width + TEST_BORDER as i16 + TEST_PAD
        );
    }

    #[test]
    fn test_leader_left_three_windows() {
        let tiling_dims = calculate_dimensions(3, false);
        assert_eq!(3, tiling_dims.len());
        println!("{tiling_dims:?}");
        // Y offset same for first two
        let expected_y = TEST_STATUS_HEIGHT + TEST_PAD;
        assert_eq!(expected_y, tiling_dims[0].y);
        assert_eq!(expected_y, tiling_dims[1].y);
        let right_x = tiling_dims[0].x + tiling_dims[0].width + 2 * TEST_BORDER as i16 + TEST_PAD;
        assert_eq!(right_x, tiling_dims[1].x);
        assert_eq!(
            TEST_WIDTH as i16,
            right_x + TEST_BORDER as i16 + tiling_dims[1].width + TEST_BORDER as i16 + TEST_PAD
        );
    }

    fn calculate_dimensions(
        num_windows: usize,
        pad_on_single: bool,
    ) -> heapless::CopyVec<Dimensions, WS_WINDOW_LIMIT> {
        let size_modifiers = &[1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];
        Layout::LeftLeader
            .calculate_dimensions(
                TEST_WIDTH,
                TEST_HEIGHT,
                TEST_PAD,
                TEST_BORDER,
                TEST_STATUS_HEIGHT,
                pad_on_single,
                num_windows,
                size_modifiers,
                2.0,
                2.0,
            )
            .unwrap()
    }
}
