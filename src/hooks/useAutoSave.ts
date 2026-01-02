import { useRef, useEffect, useCallback } from "react";

/**
 * Custom hook for debounced auto-save functionality
 * @param callback - Function to call when save is triggered
 * @param delay - Debounce delay in milliseconds (default 500ms)
 */
export function useAutoSave(
  callback: () => void,
  delay: number = 500,
): { triggerSave: () => void; flushSave: () => void } {
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const callbackRef = useRef(callback);

  // Keep callback ref up to date
  useEffect(() => {
    callbackRef.current = callback;
  }, [callback]);

  // Trigger a debounced save
  const triggerSave = useCallback(() => {
    // Clear any existing timeout
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
    }

    // Set new timeout
    timeoutRef.current = setTimeout(() => {
      callbackRef.current();
      timeoutRef.current = null;
    }, delay);
  }, [delay]);

  // Immediately flush any pending save
  const flushSave = useCallback(() => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
      callbackRef.current();
    }
  }, []);

  // Cleanup on unmount - flush any pending save
  useEffect(() => {
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
        // Optionally save on unmount
        callbackRef.current();
      }
    };
  }, []);

  return { triggerSave, flushSave };
}
