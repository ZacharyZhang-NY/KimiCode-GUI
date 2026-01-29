const fs = require('fs');
const path = require('path');

const required = [
  'index.html',
  'styles.css',
  'app.js',
  path.join('vendor', 'xterm.js'),
  path.join('vendor', 'xterm.css'),
  path.join('vendor', 'addon-fit.js')
];

const missing = required.filter((file) => !fs.existsSync(path.join(__dirname, file)));
if (missing.length) {
  console.error('Missing UI assets:', missing.join(', '));
  process.exit(1);
}

console.log('Static UI ready. No build step required.');
