.pragma library

// The label in `year` is localized display text. Only the numeric original
// release date participates in chronology; unknown dates remain last.
function compareDates(a, b, newest) {
    var x = Number(a.releaseSortKey || 0), y = Number(b.releaseSortKey || 0)
    if (!x || !y) return !x && !y ? 0 : (!x ? 1 : -1)
    return newest ? y - x : x - y
}

function yearOf(row) {
    return Math.floor(Number(row.releaseSortKey || 0) / 10000)
}

function groupNumber(row, group) {
    var year = yearOf(row)
    return group === "decade" ? Math.floor(year / 10) * 10 : year
}

function groupLabel(row, group) {
    var year = groupNumber(row, group)
    if (!year) return "#"
    return group === "decade" ? year + "–" + (year + 9) : String(year)
}

function textCompare(a, b) {
    var x = String(a || "").toLowerCase(), y = String(b || "").toLowerCase()
    return x < y ? -1 : (x > y ? 1 : 0)
}

function sortAlbums(rows, sort, group) {
    return rows.slice().sort(function (a, b) {
        var grouped = 0
        if (group === "year" || group === "decade") {
            var x = groupNumber(a, group), y = groupNumber(b, group)
            if (!x || !y) grouped = !x && !y ? 0 : (!x ? 1 : -1)
            else grouped = sort === "newest" ? y - x : x - y
        } else if (group === "artist") {
            grouped = textCompare(a.artist, b.artist)
        } else if (group === "alpha") {
            grouped = textCompare(String(a.title || "").charAt(0), String(b.title || "").charAt(0))
        }
        if (grouped) return grouped
        if (sort === "oldest" || sort === "newest")
            return compareDates(a, b, sort === "newest")
        if (sort === "artist-asc") return textCompare(a.artist, b.artist)
        if (sort === "title-asc" || sort === "title-desc") {
            var title = textCompare(a.title, b.title)
            return sort === "title-desc" ? -title : title
        }
        // Default retains server/feed order; alphabetical groups keep their
        // established alphabetical order within each group.
        return group === "artist" || group === "alpha" ? textCompare(a.title, b.title) : 0
    })
}
