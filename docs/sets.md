# Televisions this has been pointed at

Every number in this project was taken on one set in one room. One row is not
a survey, and this page says so above the table instead of under it.

What this project cannot find out on its own is what a different tube, a
different converter or a different card does with the same timings. If you
have a set, [say what it did](https://github.com/stefanomainardi/omacrt/issues/new?template=set-report.yml).
A set that refuses to lock is as useful as one that does.

| Set | In front of it | Card | Locked | Follows a stretched blanking down to |
| --- | --- | --- | --- | --- |
| Bang & Olufsen BeoCenter 1, 1998 | RGB-Pi 2 over HDMI, RGB SCART | RX 7700/7800 XT, amdgpu | yes, 15.731 kHz, 240p and 288p | **55 Hz**, then a cliff: 15% shorter at 54 and no worse at 50 |

## What the last column means

A television's horizontal rate must not move, but its vertical one may, so a
refresh can be changed by stretching the vertical blanking alone and never
touching the line rate. How far a set follows that before its picture starts
losing height is a property of the set, and the one number here that cannot be
derived from anything else.

On the set above it was measured from film: the picture keeps its height at 55
Hz and is 15% shorter at 54, and it stays 15% shorter all the way down to 50
instead of shrinking further. A regulation drops out, and an amplitude does
not follow a period, so the configuration holds at 55 with no margin below it. Six seconds back at 60 Hz and the picture was still 3.7%
short and climbing, so the recovery is slow as well.

You do not need a camera to report it. Run `omacrt rate 58`, then 56, 55, 54,
52, 50, and say the lowest one where the picture still filled the screen.

## Before you point anything at a set you care about

A television's horizontal deflection is a circuit tuned for one line rate and
under no obligation to work at another. What a given set does when it is given
another is a property of that set: some lose sync and roll, some blank, some
shut themselves down, and some have no protection worth the name. This project
has tested none of that on any television and does not intend to.

[What the guard refuses](flyback.md#safety) is written down so it can be
checked. Read it first.
