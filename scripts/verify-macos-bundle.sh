#!/bin/sh
set -eux
app_bundle=$1
expected_arch=$2
for binary in open-island open-islandd; do
    executable="$app_bundle/Contents/MacOS/$binary"
    test -x "$executable"
    test "$(lipo -archs "$executable")" = "$expected_arch"
    codesign --verify --strict "$executable"
done
codesign --verify --deep --strict "$app_bundle"
for sound in device-added complete message dialog-warning suspend-error; do
    test -s "$app_bundle/Contents/Resources/sounds/$sound.wav"
done
/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$app_bundle/Contents/Info.plist"
test "$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$app_bundle/Contents/Info.plist")" = "12.0"
