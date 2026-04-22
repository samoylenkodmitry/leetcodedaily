# LeetCode Daily Composer

Desktop GUI app built with `cranpose 0.0.59` for assembling daily LeetCode post markdown and rendering a shareable code card image.

## What It Does

- Fills the markdown template for the daily post
- Copies the final markdown to the clipboard
- Generates a preview image from the bundled background and QR assets
- Saves the final image as WebP into `output/`

## Assets

- Background: `assets/background.jpg`
- QR overlay: `assets/qr-overlay.png`

## Run

```bash
cargo run
```

## Test

```bash
cargo test
```

## Output

- Preview PNG: `output/preview-latest.png`
- Saved WebP: `output/<date>-<problem>.webp`
