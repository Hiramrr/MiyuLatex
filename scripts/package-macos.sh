#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_dir"
cargo build --release
app_version=$(target/release/miyu --version)
app_version=${app_version#miyu }
contents="$project_dir/dist/MiyuLaTeX.app/Contents"
mkdir -p "$contents/MacOS" "$contents/Resources"
# Sobrescribir el binario en su sitio deja la firma anterior en caché y macOS
# mata el proceso al abrirlo. Se borra antes de copiar y se firma de nuevo.
rm -f "$contents/MacOS/MiyuLaTeX"
cp target/release/miyu "$contents/MacOS/MiyuLaTeX"
cp assets/icon.icns "$contents/Resources/icon.icns"
xcrun actool assets/MiyuTeX.icon --compile "$contents/Resources" \
    --app-icon MiyuTeX --platform macosx --target-device mac \
    --minimum-deployment-target 11.0 \
    --output-partial-info-plist target/miyutex-icon-info.plist \
    --output-format human-readable-text
cp assets/file-icons-LICENSE.txt "$contents/Resources/file-icons-LICENSE.txt"
rm -f "$contents/Resources/miyu" "$contents/Resources/MiyuLaTeX.command"

cat > "$contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
    <key>CFBundleName</key><string>MiyuLaTeX</string>
    <key>CFBundleDisplayName</key><string>MiyuLaTeX</string>
    <key>CFBundleIdentifier</key><string>local.miyulatex.editor</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleExecutable</key><string>MiyuLaTeX</string>
    <key>CFBundleIconFile</key><string>icon.icns</string>
    <key>CFBundleIconName</key><string>MiyuTeX</string>
    <key>CFBundleShortVersionString</key><string>$app_version</string>
    <key>CFBundleVersion</key><string>$app_version</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSPrincipalClass</key><string>NSApplication</string>
</dict></plist>
EOF
plutil -lint "$contents/Info.plist"
codesign --force --sign - "$project_dir/dist/MiyuLaTeX.app"
printf '%s\n' "$project_dir/dist/MiyuLaTeX.app"
