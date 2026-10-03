// Uso: node scripts/generate-file-icons.cjs /ruta/al/paquete/material-icon-theme
// Requiere sharp para generar los PNG. La aplicación no lo necesita.
const fs = require('node:fs');
const path = require('node:path');
const sharp = require('sharp');

async function main() {
  const source = process.argv[2];
  if (!source) throw new Error('Falta la carpeta del paquete material-icon-theme');
  const upstream = JSON.parse(fs.readFileSync(path.join(source, 'dist/material-icons.json')));
  const pkg = JSON.parse(fs.readFileSync(path.join(source, 'package.json')));
  const normalize = (entries) => Object.fromEntries(
    Object.entries(entries).map(([name, icon]) => [name.toLowerCase(), icon]),
  );
  const associations = (theme) => ({
    extensions: normalize(theme.fileExtensions),
    names: normalize(theme.fileNames),
  });
  const base = associations(upstream);
  const light = associations(upstream.light);
  Object.assign(base.names, { 'cargo.toml': 'rust', 'cargo.lock': 'rust' });
  const icons = [...new Set([
    upstream.file,
    ...Object.values(base.extensions), ...Object.values(base.names),
    ...Object.values(light.extensions), ...Object.values(light.names),
  ])].sort();
  const indices = new Map(icons.map((icon, i) => [icon, i]));
  const indexed = (entries) => Object.fromEntries(
    Object.entries(entries).sort(([a], [b]) => a.localeCompare(b))
      .map(([name, icon]) => [name, indices.get(icon)]),
  );
  const cell = 64;
  const columns = 32;
  const rows = Math.ceil(icons.length / columns);
  const sprites = [];
  for (const [i, icon] of icons.entries()) {
    const svg = path.resolve(source, 'dist', upstream.iconDefinitions[icon].iconPath);
    sprites.push({
      input: await sharp(svg).resize(cell - 4, cell - 4).png().toBuffer(),
      left: (i % columns) * cell + 2,
      top: Math.floor(i / columns) * cell + 2,
    });
  }
  const output = path.resolve(__dirname, '../assets');
  await sharp({ create: {
    width: columns * cell, height: rows * cell, channels: 4, background: '#00000000',
  } }).composite(sprites).png().toFile(path.join(output, 'file-icons.png'));
  fs.writeFileSync(path.join(output, 'file-icons.json'), JSON.stringify({
    source: `material-icon-theme ${pkg.version}`,
    cell, columns, rows, icons, file: indices.get(upstream.file),
    extensions: indexed(base.extensions), names: indexed(base.names),
    light: { extensions: indexed(light.extensions), names: indexed(light.names) },
  }, null, 2) + '\n');
  fs.writeFileSync(path.join(output, 'file-icons-LICENSE.txt'),
    `File icons from Material Icon Theme ${pkg.version}\n` +
    'https://github.com/material-extensions/vscode-material-icon-theme\n' +
    `https://www.npmjs.com/package/material-icon-theme/v/${pkg.version}\n\n` +
    fs.readFileSync(path.join(source, 'LICENSE'), 'utf8'));
  console.log(`${icons.length} iconos, ${Object.keys(base.extensions).length} extensiones, ${Object.keys(base.names).length} nombres`);
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
