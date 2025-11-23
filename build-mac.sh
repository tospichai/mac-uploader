#!/bin/bash

cargo build --release

APP="MacUploader.app"
BIN="./target/release/mac-uploader"

mkdir -p $APP/Contents/MacOS
mkdir -p $APP/Contents/Resources

cp $BIN $APP/Contents/MacOS/

cat <<EOF > $APP/Contents/Info.plist
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
"http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <key>CFBundleName</key>
    <string>Foldex</string>
    <key>CFBundleIdentifier</key>
    <string>com.khai.foldex</string>
    <key>CFBundleExecutable</key>
    <string>foldex</string>
    <key>LSUIElement</key>
    <true/>
  </dict>
</plist>
EOF
