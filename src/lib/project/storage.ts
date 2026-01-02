import fs from "fs";
import path from "path";
import type { ProjectFile } from "./types.js";
import { validateProject } from "./serializer.js";

const PROJECT_FILE_NAME = "project.json";

/**
 * Check if a path contains a valid project
 */
export function isValidProject(projectPath: string): boolean {
  const projectFile = path.join(projectPath, PROJECT_FILE_NAME);
  return fs.existsSync(projectFile);
}

/**
 * Load a project from disk
 */
export function loadProject(projectPath: string): ProjectFile | null {
  const projectFile = path.join(projectPath, PROJECT_FILE_NAME);

  if (!fs.existsSync(projectFile)) {
    return null;
  }

  try {
    const content = fs.readFileSync(projectFile, "utf-8");
    const parsed = JSON.parse(content);

    if (!validateProject(parsed)) {
      console.error("Invalid project file format");
      return null;
    }

    return parsed;
  } catch (error) {
    console.error("Failed to load project:", error);
    return null;
  }
}

/**
 * Save a project to disk (atomic write)
 */
export function saveProject(projectPath: string, project: ProjectFile): void {
  const projectFile = path.join(projectPath, PROJECT_FILE_NAME);
  const tempFile = path.join(projectPath, `.${PROJECT_FILE_NAME}.tmp`);

  try {
    // Ensure directory exists
    if (!fs.existsSync(projectPath)) {
      fs.mkdirSync(projectPath, { recursive: true });
    }

    // Write to temp file first
    const content = JSON.stringify(project, null, 2);
    fs.writeFileSync(tempFile, content, "utf-8");

    // Atomic rename
    fs.renameSync(tempFile, projectFile);
  } catch (error) {
    // Clean up temp file if it exists
    if (fs.existsSync(tempFile)) {
      fs.unlinkSync(tempFile);
    }
    throw error;
  }
}

/**
 * Create a new project directory structure
 */
export function createProjectDirectory(projectPath: string): void {
  // Create main project directory
  if (!fs.existsSync(projectPath)) {
    fs.mkdirSync(projectPath, { recursive: true });
  }

  // Create samples subdirectory
  const samplesDir = path.join(projectPath, "samples");
  if (!fs.existsSync(samplesDir)) {
    fs.mkdirSync(samplesDir, { recursive: true });
  }
}

/**
 * Copy default samples from app to project
 * @param projectPath - The project directory path
 * @param appSamplesDir - The app's default samples directory
 */
export function copyDefaultSamples(
  projectPath: string,
  appSamplesDir: string,
): void {
  const projectSamplesDir = path.join(projectPath, "samples");

  if (!fs.existsSync(appSamplesDir)) {
    console.warn("App samples directory not found:", appSamplesDir);
    return;
  }

  // Recursively copy all samples
  copyDirectoryRecursive(appSamplesDir, projectSamplesDir);
}

/**
 * Recursively copy a directory
 */
function copyDirectoryRecursive(src: string, dest: string): void {
  // Create destination directory if it doesn't exist
  if (!fs.existsSync(dest)) {
    fs.mkdirSync(dest, { recursive: true });
  }

  const entries = fs.readdirSync(src, { withFileTypes: true });

  for (const entry of entries) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);

    if (entry.isDirectory()) {
      copyDirectoryRecursive(srcPath, destPath);
    } else {
      // Only copy if destination doesn't exist (don't overwrite)
      if (!fs.existsSync(destPath)) {
        fs.copyFileSync(srcPath, destPath);
      }
    }
  }
}

/**
 * Generate a default project name with timestamp
 */
export function generateDefaultProjectName(): string {
  const now = new Date();
  const timestamp = now.toISOString().slice(0, 19).replace(/[T:]/g, "-");
  return `untitled-${timestamp}`;
}

/**
 * Get the project's samples directory path
 */
export function getProjectSamplesDir(projectPath: string): string {
  return path.join(projectPath, "samples");
}

/**
 * Extract project name from path
 */
export function getProjectNameFromPath(projectPath: string): string {
  return path.basename(projectPath);
}
