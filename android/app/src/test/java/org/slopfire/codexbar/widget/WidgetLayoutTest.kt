package org.slopfire.codexbar.widget

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class WidgetLayoutTest {

    @Test
    fun unknownHeightGetsTheBareCard() {
        val plan = WidgetLayout.plan(Float.NaN, 8f, marker = true, siblingCount = 2, tileWindows = List(3) { 2 })
        assertEquals(HeroPlan(showMeta = false, siblings = 0, tiles = 0, bigNumber = false), plan)
    }

    @Test
    fun twoCellSlotShowsOneOtherProvider() {
        // ~205dp outer, 181dp inner: the screenshot that motivated the redesign.
        val plan = WidgetLayout.plan(181f, 8f, marker = true, siblingCount = 0, tileWindows = List(1) { 2 })
        assertTrue(plan.showMeta)
        assertEquals(1, plan.tiles)
    }

    @Test
    fun rowsAlternateSiblingThenProvider() {
        val plan = WidgetLayout.plan(260f, 8f, marker = true, siblingCount = 2, tileWindows = List(2) { 2 })
        // base 110 + meta 18 = 128; 132 left: sibling 26, tile 46, sibling 26 -> 34 left.
        assertEquals(2, plan.siblings)
        assertEquals(1, plan.tiles)
        assertFalse(plan.bigNumber)
    }

    @Test
    fun spareHeightGrowsTheNumberInsteadOfLeavingAHole() {
        val plan = WidgetLayout.plan(290f, 8f, marker = true, siblingCount = 1, tileWindows = emptyList())
        assertEquals(1, plan.siblings)
        assertTrue(plan.bigNumber)
    }

    @Test
    fun shortSlotDropsTheMetaLine() {
        val plan = WidgetLayout.plan(120f, 8f, marker = true, siblingCount = 1, tileWindows = List(1) { 2 })
        assertFalse(plan.showMeta)
        assertEquals(0, plan.siblings + plan.tiles)
    }

    @Test
    fun tallSlotGivesEveryTileItsOwnBarBeforeGrowingTheHero() {
        // 4x4-ish: base 128 + three tiles 138 = 266, leaving ~247; six labelled
        // window rows cost 144 and still leave the hero its bigger number.
        val plan = WidgetLayout.plan(513f, 8f, marker = true, siblingCount = 0, tileWindows = List(3) { 2 })
        assertEquals(3, plan.tiles)
        assertTrue(plan.expandTiles)
        assertTrue(plan.bigNumber)
    }

    @Test
    fun tilesStayCompactWhenTheBarsWouldNotFit() {
        val plan = WidgetLayout.plan(181f, 8f, marker = true, siblingCount = 0, tileWindows = List(1) { 2 })
        assertFalse(plan.expandTiles)
    }

    @Test
    fun expansionIsPricedPerWindow() {
        // Same height, but each provider has three windows: 9 rows = 216 > 247 - 36.
        val plan = WidgetLayout.plan(513f, 8f, marker = true, siblingCount = 0, tileWindows = List(3) { 3 })
        assertEquals(3, plan.tiles)
        assertFalse(plan.expandTiles)
    }

    @Test
    fun pairsUseFullLabelsWhenTheyFit() {
        val (labels, shown) = WidgetLayout.fitPairs(400f, listOf("month", "week", "5h"), listOf("0%", "55%", "80%"))
        assertEquals(listOf("month", "week", "5h"), labels)
        assertEquals(3, shown)
    }

    @Test
    fun pairsAbbreviateBeforeDroppingAndLeaveRoomForTheCount() {
        // Full: 67 + 68 + 55 = 190 > 170. Short: 46 + 55 + 55 = 156 fits.
        val (labels, shown) = WidgetLayout.fitPairs(170f, listOf("month", "week", "5h"), listOf("0%", "55%", "80%"))
        assertEquals(listOf("mo", "wk", "5h"), labels)
        assertEquals(3, shown)
        // Too narrow even short: two pairs plus the `+1` reserve.
        val (_, fewer) = WidgetLayout.fitPairs(140f, listOf("month", "week", "5h"), listOf("0%", "55%", "80%"))
        assertEquals(2, fewer)
    }
}
