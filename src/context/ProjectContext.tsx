import {
  createContext,
  useContext,
  useEffect,
  useRef,
  type ReactNode,
} from "react";
import { useSequencer } from "./SequencerContext.js";
import { useAutoSave } from "../hooks/useAutoSave.js";
import { serializeProject } from "../lib/project/serializer.js";
import { saveProject } from "../lib/project/storage.js";

interface ProjectContextType {
  projectPath: string;
  projectName: string;
  save: () => void;
}

const ProjectContext = createContext<ProjectContextType | null>(null);

interface ProjectProviderProps {
  children: ReactNode;
  projectPath: string;
  projectName: string;
  createdAt?: Date;
}

export function ProjectProvider({
  children,
  projectPath,
  projectName,
  createdAt,
}: ProjectProviderProps) {
  const { getSerializableState } = useSequencer();
  const createdAtRef = useRef(createdAt ?? new Date());

  // Save function that serializes and writes to disk
  const performSave = () => {
    try {
      const state = getSerializableState();
      const projectFile = serializeProject(
        state,
        projectName,
        createdAtRef.current,
      );
      saveProject(projectPath, projectFile);
    } catch (error) {
      console.error("Failed to save project:", error);
    }
  };

  // Set up auto-save with 500ms debounce
  const { triggerSave, flushSave } = useAutoSave(performSave, 500);

  // Track previous state to detect changes
  const prevStateRef = useRef<string>("");

  // Watch for state changes and trigger auto-save
  useEffect(() => {
    const state = getSerializableState();
    const stateJson = JSON.stringify(state);

    // Only save if state has actually changed
    if (prevStateRef.current && prevStateRef.current !== stateJson) {
      triggerSave();
    }

    prevStateRef.current = stateJson;
  }, [getSerializableState, triggerSave]);

  // Flush save on unmount
  useEffect(() => {
    return () => {
      flushSave();
    };
  }, [flushSave]);

  return (
    <ProjectContext.Provider
      value={{
        projectPath,
        projectName,
        save: performSave,
      }}
    >
      {children}
    </ProjectContext.Provider>
  );
}

export function useProject() {
  const context = useContext(ProjectContext);
  if (!context) {
    throw new Error("useProject must be used within ProjectProvider");
  }
  return context;
}
