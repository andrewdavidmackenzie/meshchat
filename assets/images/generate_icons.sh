#!/bin/bash
# Generate macOS .icns and high-res PNG icons from SVG
# Requires: Inkscape, ImageMagick, or rsvg-convert (from librsvg)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SVG_FILE="$SCRIPT_DIR/icon.svg"
ICONSET_DIR="$SCRIPT_DIR/icon.iconset"

# Check for SVG converter
if command -v rsvg-convert &> /dev/null; then
    CONVERTER="rsvg"
elif command -v inkscape &> /dev/null; then
    CONVERTER="inkscape"
elif command -v magick &> /dev/null; then
    CONVERTER="magick"
elif command -v convert &> /dev/null; then
    CONVERTER="convert"
else
    echo "Error: No SVG converter found."
    echo "Please install one of: librsvg, inkscape, or imagemagick"
    echo ""
    echo "  brew install librsvg    # Recommended - fastest"
    echo "  brew install inkscape"
    echo "  brew install imagemagick"
    exit 1
fi

echo "Using converter: $CONVERTER"
echo "Source: $SVG_FILE"

# Function to convert SVG to PNG at specific size
svg_to_png() {
    local size=$1
    local output=$2

    case $CONVERTER in
        rsvg)
            rsvg-convert -w "$size" -h "$size" "$SVG_FILE" -o "$output"
            ;;
        inkscape)
            inkscape --export-type=png --export-filename="$output" -w "$size" -h "$size" "$SVG_FILE" 2>/dev/null
            ;;
        magick)
            magick -background none -density 300 "$SVG_FILE" -resize "${size}x${size}" "$output"
            ;;
        convert)
            convert -background none -density 300 "$SVG_FILE" -resize "${size}x${size}" "$output"
            ;;
    esac
}

# Create iconset directory
rm -rf "$ICONSET_DIR"
mkdir -p "$ICONSET_DIR"

echo "Generating icon sizes..."

# Generate all required sizes for macOS .iconset
# Format: icon_WxH.png for 1x, icon_WxH@2x.png for retina
sizes=(16 32 128 256 512)

for size in "${sizes[@]}"; do
    echo "  ${size}x${size}..."
    svg_to_png "$size" "$ICONSET_DIR/icon_${size}x${size}.png"

    # Generate @2x version (double resolution)
    double=$((size * 2))
    if [ $double -le 1024 ]; then
        echo "  ${size}x${size}@2x (${double}px)..."
        svg_to_png "$double" "$ICONSET_DIR/icon_${size}x${size}@2x.png"
    fi
done

# Generate 512@2x (1024px) separately
echo "  512x512@2x (1024px)..."
svg_to_png 1024 "$ICONSET_DIR/icon_512x512@2x.png"

# Generate main icon.png at high resolution
echo "Generating high-res icon.png (1024px)..."
svg_to_png 1024 "$SCRIPT_DIR/icon.png"

# Generate icon.icns using iconutil (macOS built-in)
echo "Creating icon.icns..."
iconutil -c icns "$ICONSET_DIR" -o "$SCRIPT_DIR/icon.icns"

# Clean up iconset directory (optional - comment out to keep)
# rm -rf "$ICONSET_DIR"

echo ""
echo "Done! Generated:"
echo "  - $SCRIPT_DIR/icon.png (1024x1024)"
echo "  - $SCRIPT_DIR/icon.icns (all sizes)"
echo "  - $ICONSET_DIR/ (intermediate files)"
