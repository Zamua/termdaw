import { render } from "ink";
import path from "path";
import App from "./src/App.js";
import {
  isValidProject,
  loadProject,
  createProjectDirectory,
  copyDefaultSamples,
  generateDefaultProjectName,
  getProjectSamplesDir,
  getProjectNameFromPath,
} from "./src/lib/project/index.js";
import { deserializeProject } from "./src/lib/project/serializer.js";
import { setProjectSamplesDir } from "./src/lib/audio.js";
import type { InitialState } from "./src/context/SequencerContext.js";

// Parse CLI arguments
const args = process.argv.slice(2);
const projectArg = args[0];

// Resolve project path
const projectName = projectArg || generateDefaultProjectName();
const projectPath = path.resolve(projectName);

// Initialize project
let initialState: InitialState = null;
let createdAt: Date | undefined;

if (isValidProject(projectPath)) {
  // Load existing project
  const projectFile = loadProject(projectPath);
  if (projectFile) {
    initialState = deserializeProject(projectFile);
    createdAt = new Date(projectFile.createdAt);
    console.log(`Loading project: ${projectPath}`);
  } else {
    console.error(`Failed to load project: ${projectPath}`);
    process.exit(1);
  }
} else {
  // Create new project
  console.log(`Creating new project: ${projectPath}`);
  createProjectDirectory(projectPath);

  // Copy default samples from app's samples directory
  const appSamplesDir = path.join(import.meta.dir, "samples");
  copyDefaultSamples(projectPath, appSamplesDir);

  createdAt = new Date();
}

// Configure audio to use project's samples directory
setProjectSamplesDir(getProjectSamplesDir(projectPath));

// Use alternate screen buffer for fullscreen experience
process.stdout.write("\x1b[?1049h"); // Enter alternate screen
process.stdout.write("\x1b[?25l"); // Hide cursor

const instance = render(
  <App
    projectPath={projectPath}
    projectName={getProjectNameFromPath(projectPath)}
    initialState={initialState}
    createdAt={createdAt}
  />,
);

instance.waitUntilExit().then(() => {
  process.stdout.write("\x1b[?25h"); // Show cursor
  process.stdout.write("\x1b[?1049l"); // Exit alternate screen
});
