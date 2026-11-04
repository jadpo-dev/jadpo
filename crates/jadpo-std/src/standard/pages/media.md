# Colour

A colour, written `#1e6fd9`, or `#1e6fd980` when it isn't fully opaque.

Use it for brand colours and themes. `colour.contrast` tells you whether two are
readable together, and `colour.mix` blends them.

```jadpo
Colour { red: 30 green: 111 blue: 217 alpha: 255 }
```

- Each part runs from 0 to 255. `alpha` of 255 is fully opaque.
- It is lower case with a `#`, in sRGB.

# ImageDimensions

An image's width and height in pixels: `1920x1080`.

Use it for photos and pictures. `media.fit` scales one to fit inside another. For
video, use `VideoResolution`. For a physical box, use `Dimensions`.

```jadpo
ImageDimensions { width: 1920 height: 1080 }
```

- Each side must be at least 1.

# AspectRatio

Width to height in whole numbers: `16:9`.

Use it for screens, images and video. `16:9` and `32:18` stay two different values.
For a ratio of any two numbers, use `Ratio`.

```jadpo
AspectRatio { width: 16 height: 9 }
```

- Decimals are refused, so `1.85:1` is written `185:100` here.

# Rect

A rectangle by its top-left corner and its size, written like an SVG `viewBox`: `0 0 300 200`.

Use it for a crop or a region of an image. For space around an edge, use `Insets`.

```jadpo
Rect { x: 0.0 y: 0.0 width: 300.0 height: 200.0 }
```

- `x` and `y` can be below zero, but the width and height can't.

# Insets

Space inside each edge, written in CSS's order: `8 16 8 16`.

Use it for padding in a layout. For a rectangle, use `Rect`.

```jadpo
Insets { top: 8.0 right: 16.0 bottom: 8.0 left: 16.0 }
```

- None can be below zero, and text must give all four.

# PaperSize

A paper size: `A4`, `Letter` and so on.

Use it for printing and PDFs.

```jadpo
PaperSize.a4
```

# ExifOrientation

How a stored photo must be turned to look upright, as a camera records it.

Use it to display or process photos. `top_left` is `1`, which needs no turning.

```jadpo
ExifOrientation.right_top
```

- It is written as the number a camera records, from `1` to `8`.

# AltText

What an image shows, for someone who can't see it.

Use it beside each image a person might read about. An image with nothing to say has
`none`, not empty text. For a long caption, use `Text`.

```jadpo
AltText("A red bicycle leaning against a brick wall")
```

- It is one line of up to 1,000 characters.

# FrameRate

Frames a second, as an exact fraction: `25/1`, or `30000/1001` for 29.97.

Use it for video. Video rates aren't round numbers, which is why this is a fraction.
For anything else per second, use `Frequency`.

```jadpo
FrameRate { numerator: 25 denominator: 1 }
```

- `29.97` is refused. Write the fraction.

# VideoResolution

A video's frame size in pixels: `1920x1080`.

Use it for video. Give the pixels, not a name like "4K". For a still image, use
`ImageDimensions`.

```jadpo
VideoResolution { width: 1920 height: 1080 }
```

# VideoCodec

A video format as browsers name it: `avc1.42001E`, `vp09.00.10.08`.

Use it to tell a player which format a video is in. For sound, use `AudioCodec`.

```jadpo
VideoCodec("avc1.42001E")
```

- Casual names such as `h264` are refused, since a player needs the whole name.

# AudioCodec

An audio format as browsers name it: `mp4a.40.2`, `opus`.

Use it to tell a player which format a sound is in. For video, use `VideoCodec`.

```jadpo
AudioCodec("opus")
```

- Casual names such as `aac` are refused: AAC is `mp4a.40.2`.

# Timecode

An address of one frame in a video: hours, minutes, seconds and frames.

Use it for video editing. It holds its frame rate, so a time on its own isn't enough.
For a plain amount of time, use `Duration`.

```jadpo
Timecode {
    hours: 1
    minutes: 0
    seconds: 0
    frames: 0
    rate: FrameRate { numerator: 25 denominator: 1 }
    drop_frame: false
}
```

- `drop_frame` works only at 30000/1001 or 60000/1001.
- It has no single text form, so `text.parse` won't take it.

# SubtitleCue

A timed caption: when it starts and ends, and its text.

Use it for subtitles, as WebVTT holds them.

```jadpo
SubtitleCue { start: 1s end: 4s text: "Never drink liquid nitrogen." }
```

- Times count from the start of the media. The end must be after the start, and the
  text can't hold `-->`.

# ContentRating

An age rating and the body that gave it: `BBFC 12A`.

Use it for films and games. The rating has to be one of that body's own categories, so
`BBFC PG-13` is refused.

```jadpo
ContentRating { scheme: ContentRatingScheme.bbfc rating: "12A" }
```

# ContentRatingScheme

A body that rates films or games for age: `BBFC`, `MPA`, `PEGI` or `ESRB`.

Use it as part of a `ContentRating`, or to choose which ratings to show in a country.

```jadpo
ContentRatingScheme.bbfc
```
