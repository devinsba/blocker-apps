package dev.blocker.core

import java.time.Clock
import java.time.DayOfWeek
import java.time.Instant
import java.time.LocalDateTime
import java.time.ZoneId
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

class WeekTimeTest {
    @Test
    fun mondayIsZeroAndSundayIsSix() {
        assertEquals(WeekTime(0, 0), WeekTime.of(LocalDateTime.of(2026, 11, 16, 0, 0))) // Monday
        assertEquals(WeekTime(6, 23 * 60 + 59), WeekTime.of(LocalDateTime.of(2026, 11, 22, 23, 59))) // Sunday
    }

    @Test
    fun ofDayHourMinute() {
        val t = WeekTime.of(DayOfWeek.WEDNESDAY, 9, 30)
        assertEquals(2, t.day)
        assertEquals(570, t.minute)
        assertEquals(9, t.hour)
        assertEquals(30, t.minuteOfHour)
        assertEquals(DayOfWeek.WEDNESDAY, t.dayOfWeek)
        assertEquals("wed 09:30", t.toString())
    }

    @Test
    fun nowUsesTheClockTimeZone() {
        // 2026-11-16T23:30Z is Monday in UTC but Tuesday 08:30 in Tokyo.
        val instant = Instant.parse("2026-11-16T23:30:00Z")
        assertEquals(WeekTime(0, 23 * 60 + 30), WeekTime.now(Clock.fixed(instant, ZoneId.of("UTC"))))
        assertEquals(WeekTime(1, 8 * 60 + 30), WeekTime.now(Clock.fixed(instant, ZoneId.of("Asia/Tokyo"))))
    }

    @Test
    fun rejectsOutOfRangeValues() {
        assertThrows(IllegalArgumentException::class.java) { WeekTime(-1, 0) }
        assertThrows(IllegalArgumentException::class.java) { WeekTime(7, 0) }
        assertThrows(IllegalArgumentException::class.java) { WeekTime(0, 1440) }
        assertThrows(IllegalArgumentException::class.java) { WeekTime(0, -1) }
        assertThrows(IllegalArgumentException::class.java) { WeekTime.of(DayOfWeek.MONDAY, 24, 0) }
    }
}
