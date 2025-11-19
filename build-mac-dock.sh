#!/usr/bin/env bash

set -euo pipefail

###################################
# CONFIG แก้ให้ตรงกับโปรเจคตัวเอง
###################################

# ชื่อ binary ที่ Cargo build ให้ (ดูจาก [[bin]] หรือ name ใน Cargo.toml)
BINARY_NAME="mac-uploader"

# ชื่อแอปที่จะแสดงใน Finder / Dock
APP_NAME="MacUploader"

# Bundle ID (ตั้งเองได้ แต่ควรไม่ซ้ำ)
BUNDLE_ID="com.khai.mac-uploader-v1"

# profile ที่ใช้ build (ปกติใช้ release)
BUILD_PROFILE="release"

# ไดเรกทอรีเอาท์พุต .app
DIST_DIR="dist"

# ถ้ามีไอคอน .icns ให้ใส่ path ไว้ตรงนี้ (ไม่มีก็ปล่อยว่างได้)
ICON_FILE="assets/app-icon.icns"


###################################
# เริ่มทำงานจริง
###################################

echo "🚀 Building Rust binary (${BUILD_PROFILE})..."
cargo build --profile "${BUILD_PROFILE}"

BIN_PATH="target/${BUILD_PROFILE}/${BINARY_NAME}"

if [ ! -f "${BIN_PATH}" ]; then
  echo "❌ ไม่พบ binary ที่ ${BIN_PATH}"
  echo "   ตรวจสอบว่า BINARY_NAME ตรงกับที่ Cargo build ให้มาหรือยัง"
  exit 1
fi

APP_DIR="${DIST_DIR}/${APP_NAME}.app"
CONTENTS_DIR="${APP_DIR}/Contents"
MACOS_DIR="${CONTENTS_DIR}/MacOS"
RESOURCES_DIR="${CONTENTS_DIR}/Resources"

echo "📁 Preparing app bundle at: ${APP_DIR}"

# ลบของเก่า (ถ้ามี)
rm -rf "${APP_DIR}"
mkdir -p "${MACOS_DIR}"
mkdir -p "${RESOURCES_DIR}"

echo "📦 Copying binary..."
cp "${BIN_PATH}" "${MACOS_DIR}/${BINARY_NAME}"
chmod +x "${MACOS_DIR}/${BINARY_NAME}"

echo "📝 Creating Info.plist..."
cat > "${CONTENTS_DIR}/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
"http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
  <dict>
    <!-- ชื่อที่จะแสดงในเมนูบาร์ / About -->
    <key>CFBundleName</key>
    <string>${APP_NAME}</string>

    <!-- Bundle identifier -->
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>

    <!-- เวอร์ชัน (เซ็ตง่าย ๆ ไว้ก่อน) -->
    <key>CFBundleShortVersionString</key>
    <string>1.0.1</string>
    <key>CFBundleVersion</key>
    <string>2</string>

    <!-- binary หลักที่จะรัน -->
    <key>CFBundleExecutable</key>
    <string>${BINARY_NAME}</string>

    <!-- ทำให้เป็นแอปปกติ แสดงใน Dock + Cmd+Tab -->
    <key>CFBundlePackageType</key>
    <string>APPL</string>

    <!-- อย่าตั้งเป็น true ถ้าอยากให้ขึ้น Dock -->
    <key>LSUIElement</key>
    <false/>

    <!-- รองรับ HiDPI -->
    <key>NSHighResolutionCapable</key>
    <true/>
EOF

# ถ้ามี ICON_FILE ให้ใส่เพิ่ม
if [ -n "${ICON_FILE}" ] && [ -f "${ICON_FILE}" ]; then
  ICON_BASENAME=$(basename "${ICON_FILE}")
  ICON_NAME="${ICON_BASENAME%.*}"  # ตัดนามสกุลออก เช่น myapp.icns -> myapp
  echo "🎨 Copying icon: ${ICON_FILE}"
  cp "${ICON_FILE}" "${RESOURCES_DIR}/${ICON_BASENAME}"

  cat >> "${CONTENTS_DIR}/Info.plist" <<EOF
    <key>CFBundleIconFile</key>
    <string>${ICON_NAME}</string>
    <key>CFBundleIconName</key>
    <string>${ICON_NAME}</string>
EOF
fi

# ปิด plist
cat >> "${CONTENTS_DIR}/Info.plist" <<EOF
  </dict>
</plist>
EOF

echo "✅ Done!"
echo "👉 แอปของคุณอยู่ที่: ${APP_DIR}"
echo "   ดับเบิลคลิก .app นี้ได้เลย จะขึ้นใน Dock + Cmd+Tab แบบแอปปกติ"
