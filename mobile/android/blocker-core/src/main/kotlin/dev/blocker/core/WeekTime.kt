package dev.blocker.core

import java.time.Clock
import java.time.DayOfWeek
import java.time.LocalDateTime

/**
 * A point in the weekly schedule: [day] 0 = Monday .. 6 = Sunday and
 * [minute] minutes since local midnight (0..1439).
 *
 * Times are computed in Kotlin (with the device time zone) and always passed
 * explicitly to the Rust core rather than letting it read the clock itself.
 */
data class WeekTime(val day: Int, val minute: Int) {
    init {
        require(day in 0..6) { "day must be 0 (Monday) .. 6 (Sunday), was $day" }
        require(minute in 0 until MINUTES_PER_DAY) { "minute must be 0..1439, was $minute" }
    }

    val dayOfWeek: DayOfWeek get() = DayOfWeek.of(day + 1)
    val hour: Int get() = minute / 60
    val minuteOfHour: Int get() = minute % 60

    override fun toString(): String =
        "%s %02d:%02d".format(dayOfWeek.name.take(3).lowercase(), hour, minuteOfHour)

    companion object {
        const val MINUTES_PER_DAY = 24 * 60

        @JvmStatic
        @JvmOverloads
        fun now(clock: Clock = Clock.systemDefaultZone()): WeekTime = of(LocalDateTime.now(clock))

        @JvmStatic
        fun of(dateTime: LocalDateTime): WeekTime =
            WeekTime(dateTime.dayOfWeek.value - 1, dateTime.hour * 60 + dateTime.minute)

        @JvmStatic
        fun of(dayOfWeek: DayOfWeek, hour: Int, minute: Int): WeekTime {
            require(hour in 0..23) { "hour must be 0..23, was $hour" }
            require(minute in 0..59) { "minute must be 0..59, was $minute" }
            return WeekTime(dayOfWeek.value - 1, hour * 60 + minute)
        }
    }
}
