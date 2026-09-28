/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

const SWAR_ONES: u64 = 0x0101_0101_0101_0101;
const SWAR_SIGNS: u64 = 0x8080_8080_8080_8080;
const SWAR_LOW7: u64 = 0x7F7F_7F7F_7F7F_7F7F;

#[inline(always)]
pub(crate) const fn swar_splat(byte: u8) -> u64 {
    SWAR_ONES.wrapping_mul(byte as u64)
}

#[inline(always)]
pub(crate) const fn swar_zero_lanes(word: u64) -> u64 {
    !(((word & SWAR_LOW7).wrapping_add(SWAR_LOW7)) | word) & SWAR_SIGNS
}

#[inline(always)]
pub(crate) const fn swar_del_or_above(word: u64) -> u64 {
    (((word & SWAR_LOW7).wrapping_add(SWAR_ONES)) | word) & SWAR_SIGNS
}

#[inline(always)]
pub(crate) const fn swar_lane(hits: u64) -> usize {
    (hits.trailing_zeros() / 8) as usize
}
