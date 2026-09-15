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
test -s "$app_bundle/Contents/Resources/pt-BR.lproj/InfoPlist.strings"
/usr/bin/plutil -lint "$app_bundle/Contents/Resources/pt-BR.lproj/InfoPlist.strings"
test -n "$(/usr/libexec/PlistBuddy -c 'Print :NSMicrophoneUsageDescription' "$app_bundle/Contents/Info.plist")"
/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$app_bundle/Contents/Info.plist"
test "$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$app_bundle/Contents/Info.plist")" = "12.0"
python3 "$(dirname "$0")/verify-bundle-content.py" --platform macos --root "$app_bundle"
