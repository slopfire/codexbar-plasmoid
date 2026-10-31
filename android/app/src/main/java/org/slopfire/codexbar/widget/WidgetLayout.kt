package org.slopfire.codexbar.widget

/**
 * What a hero card and the tiles under it can show in a given height.
 *
 * RemoteViews cannot measure before layout, so the widget plans its content from
 * dp estimates of each block. Everything that does not fit is dropped whole —
 * a clipped row reads as a bug, a missing one does not.
 */
data class HeroPlan(
    /** The window name and reset countdown under the number. */
    val showMeta: Boolean,
    /** Other windows of the hero provider, listed inside the card. */
    val siblings: Int,
    /** Other providers, one tile each under the card. */
    val tiles: Int,
    /** The card has room to spare: use the larger display size. */
    val bigNumber: Boolean,
    /** The tiles have room for their own bar under the name. */
    val expandTiles: Boolean = false,
)

object WidgetLayout {

    /** Card padding, top plus bottom. */
    const val CARD_PADDING = 20f

    /** Badge-height header row. */
    const val HEADER = 24f

    /** The display-small number line plus the gap above it. */
    const val NUMBER = 44f

    /** The meta line and its gap. */
    const val META = 18f

    /** Gap above the bar; the bar's own height is added per call. */
    const val BAR_GAP = 8f

    /** Extra height the time-marker tick adds above and below the bar. */
    const val MARKER = 6f

    /** One window row inside the card, with its gap. */
    const val SIBLING = 26f

    /** One provider tile under the card, with its gap. */
    const val TILE = 46f

    /** What each labelled window row adds to an expanded tile. */
    const val TILE_WINDOW = 24f

    /** What display-medium adds over display-small. */
    const val BIG_NUMBER = 10f

    /** Spare height a card needs before it switches to the bigger number. */
    const val BIG_NUMBER_SLACK = 72f

    /**
     * Plans the hero layout for [innerHeight] dp of content height, or the
     * smallest layout when the height is unknown (NaN or non-positive).
     *
     * Rows are spent alternately — one sibling window, one other provider, one
     * more window, then the remaining providers — so a 2×4 slot shows a bit of
     * both rather than all of one kind.
     */
    fun plan(
        innerHeight: Float,
        barHeight: Float,
        marker: Boolean,
        siblingCount: Int,
        tileWindows: List<Int>,
        wantMeta: Boolean = true,
    ): HeroPlan {
        val height = if (innerHeight.isNaN()) 0f else innerHeight
        val bar = barHeight + if (marker) MARKER else 0f
        val base = CARD_PADDING + HEADER + NUMBER + BAR_GAP + bar
        val showMeta = wantMeta && height >= base + META
        var remaining = height - base - if (showMeta) META else 0f

        val order = buildList {
            var siblings = siblingCount.coerceAtMost(MAX_SIBLINGS)
            var tiles = tileWindows.size.coerceAtMost(MAX_TILES)
            if (siblings > 0) { add(true); siblings-- }
            if (tiles > 0) { add(false); tiles-- }
            if (siblings > 0) { add(true); siblings-- }
            repeat(tiles) { add(false) }
            repeat(siblings) { add(true) }
        }
        var siblings = 0
        var tiles = 0
        for (isSibling in order) {
            val cost = if (isSibling) SIBLING else TILE
            if (remaining < cost) break
            remaining -= cost
            if (isSibling) siblings++ else tiles++
        }
        // Spare height goes to the tiles first — a bar per provider is information,
        // a taller hero card is only emphasis.
        val expansion = tileWindows.take(tiles).sumOf { it.coerceAtLeast(1) } * TILE_WINDOW
        val expandTiles = tiles > 0 && remaining >= expansion + BIG_NUMBER_SLACK / 2
        if (expandTiles) remaining -= expansion
        return HeroPlan(
            showMeta = showMeta,
            siblings = siblings,
            tiles = tiles,
            bigNumber = remaining >= BIG_NUMBER_SLACK,
            expandTiles = expandTiles,
        )
    }

    /** Estimated advance of one body-small (12sp) character, in dp at font scale 1. */
    const val SMALL_CHAR = 6.8f

    /** Estimated advance of one label-large (14sp) character, in dp at font scale 1. */
    const val LARGE_CHAR = 8.4f

    /** Space between the label and its value, and before each pair. */
    const val PAIR_GAPS = 16f

    /** Width a `+2` overflow count needs. */
    const val OVERFLOW = 26f

    /**
     * How a compact tile's inline windows fit in [available] dp: the labels to
     * print (full, or abbreviated when the full ones do not fit) and how many of
     * the pairs make it. Values are the printed percentages, e.g. `55%`.
     */
    fun fitPairs(
        available: Float,
        labels: List<String>,
        values: List<String>,
        fontScale: Float = 1f,
    ): Pair<List<String>, Int> {
        fun width(label: String, value: String) =
            (label.length * SMALL_CHAR + value.length * LARGE_CHAR) * fontScale + PAIR_GAPS

        fun count(printed: List<String>): Int {
            var used = 0f
            printed.forEachIndexed { index, label ->
                used += width(label, values[index])
                val reserve = if (index < printed.lastIndex) OVERFLOW else 0f
                if (used + reserve > available) return index
            }
            return printed.size
        }
        val full = count(labels)
        if (full == labels.size) return labels to full
        val short = labels.map(::abbreviate)
        return short to count(short)
    }

    /** `week` → `wk`, `month` → `mo`; everything else is already short or opaque. */
    fun abbreviate(label: String): String = when (label) {
        "week" -> "wk"
        "month" -> "mo"
        else -> label
    }

    const val MAX_SIBLINGS = 3
    const val MAX_TILES = 5
}
