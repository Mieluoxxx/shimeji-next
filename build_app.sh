#!/usr/bin/env bash
# 打包 Shimeji Next.app 与 dmg。
#
#   ./build_app.sh                 本机架构
#   ./build_app.sh --universal     arm64 + x86_64 通用二进制（正式发布用）
#   ./build_app.sh --no-dmg        只出 .app
#
# 签名：默认 ad-hoc（别人下载后需右键→打开）。正式发布设置：
#   SIGN_IDENTITY="Developer ID Application: XXX (TEAMID)"
#   NOTARY_PROFILE=<xcrun notarytool store-credentials 存的 profile 名>   可选，设置后自动公证并 staple
set -euo pipefail
cd "$(dirname "$0")"

UNIVERSAL=0
MAKE_DMG=1
for arg in "$@"; do
  case "$arg" in
    --universal) UNIVERSAL=1 ;;
    --no-dmg) MAKE_DMG=0 ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "未知参数: $arg" >&2; exit 1 ;;
  esac
done

APP_NAME="Shimeji Next"
BIN=shimeji-next
BUNDLE_ID=com.wishes.shimeji-next
ASSETS=assets
SKIN_LIST=assets/bundled-skins.txt
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
OUT=target/dist
APP="$OUT/$APP_NAME.app"
SIGN_IDENTITY=${SIGN_IDENTITY:--}

echo "==> 编译 $BIN $VERSION"
if [[ $UNIVERSAL == 1 ]]; then
  rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  cargo build --release --target aarch64-apple-darwin
  cargo build --release --target x86_64-apple-darwin
  mkdir -p target/universal
  lipo -create -output "target/universal/$BIN" \
    "target/aarch64-apple-darwin/release/$BIN" \
    "target/x86_64-apple-darwin/release/$BIN"
  BIN_PATH="target/universal/$BIN"
  ARCH_TAG=universal
else
  cargo build --release
  BIN_PATH="target/release/$BIN"
  ARCH_TAG=$(uname -m)
fi

echo "==> 组装 $APP"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/skins"
cp "$BIN_PATH" "$APP/Contents/MacOS/$BIN"
cp -R "$ASSETS/i18n" "$ASSETS/icon.png" "$APP/Contents/Resources/"

if [[ -f $SKIN_LIST ]]; then
  SKINS=()
  while IFS= read -r line || [[ -n $line ]]; do
    line=${line%%#*}
    line=$(echo "$line" | xargs)
    [[ -n $line ]] && SKINS+=("$line")
  done < "$SKIN_LIST"
else
  SKINS=()
  for d in "$ASSETS"/skins/*/; do SKINS+=("$(basename "$d")"); done
fi
[[ ${#SKINS[@]} -gt 0 ]] || { echo "没有要打包的皮肤" >&2; exit 1; }
for s in "${SKINS[@]}"; do
  [[ -f "$ASSETS/skins/$s/shime1.png" ]] || { echo "皮肤不存在: $s" >&2; exit 1; }
  cp -R "$ASSETS/skins/$s" "$APP/Contents/Resources/skins/"
done
echo "    内置皮肤 ${#SKINS[@]} 个"

# 默认启用的皮肤（Settings::default）必须在包里，否则首次启动会退回列表第一个
DEFAULT_SKIN=Shimeji
if [[ ! -d "$APP/Contents/Resources/skins/$DEFAULT_SKIN" ]]; then
  echo "    警告: 默认皮肤 $DEFAULT_SKIN 没有打进包里" >&2
fi

find "$APP/Contents/Resources" \( -name Thumbs.db -o -name '*.jar' -o -name '.DS_Store' -o -name '*.bmp' -o -name '*.ico' \) -delete

ICONSET=$(mktemp -d)/AppIcon.iconset
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s "$ASSETS/icon.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) "$ASSETS/icon.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key><string>${APP_NAME}</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleExecutable</key><string>${BIN}</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>AGPL-3.0 · 基于 Shimeji-ee</string>
</dict></plist>
PLIST

echo "==> 签名 (${SIGN_IDENTITY})"
if [[ $SIGN_IDENTITY == "-" ]]; then
  codesign --force --deep --sign - "$APP"
else
  codesign --force --deep --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP"
fi
codesign --verify --deep --strict "$APP"

if [[ $MAKE_DMG == 1 ]]; then
  DMG="$OUT/ShimejiNext-${VERSION}-${ARCH_TAG}.dmg"
  echo "==> 制作 $DMG"
  STAGE=$(mktemp -d)
  cp -R "$APP" "$STAGE/"
  ln -s /Applications "$STAGE/Applications"
  rm -f "$DMG"
  # 新系统上 hdiutil create 已标记弃用；UDZO 兼容最老的目标系统
  if diskutil image create from --help >/dev/null 2>&1; then
    diskutil image create from --format UDZO --volumeName "$APP_NAME $VERSION" "$STAGE" "$DMG" >/dev/null 2>&1 || { echo "制作 dmg 失败" >&2; exit 1; }
  else
    hdiutil create -volname "$APP_NAME $VERSION" -srcfolder "$STAGE" -fs HFS+ -format UDZO -ov "$DMG" >/dev/null
  fi
  rm -rf "$STAGE"
  if [[ $SIGN_IDENTITY != "-" ]]; then
    codesign --force --sign "$SIGN_IDENTITY" --timestamp "$DMG"
    if [[ -n ${NOTARY_PROFILE:-} ]]; then
      echo "==> 公证"
      xcrun notarytool submit "$DMG" --keychain-profile "$NOTARY_PROFILE" --wait
      xcrun stapler staple "$DMG"
    fi
  fi
fi

echo "==> 完成"
du -sh "$APP" ${DMG:+"$DMG"}
lipo -archs "$APP/Contents/MacOS/$BIN"
