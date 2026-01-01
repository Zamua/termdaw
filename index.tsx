import { render } from 'ink';
import App from './src/App.js';

// Use alternate screen buffer for fullscreen experience
process.stdout.write('\x1b[?1049h'); // Enter alternate screen
process.stdout.write('\x1b[?25l');   // Hide cursor

const instance = render(<App />);

instance.waitUntilExit().then(() => {
  process.stdout.write('\x1b[?25h');   // Show cursor
  process.stdout.write('\x1b[?1049l'); // Exit alternate screen
});
