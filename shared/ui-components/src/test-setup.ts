/**
 * Vitest global setup.
 *
 * Registers @testing-library/jest-dom matchers (toBeInTheDocument, etc.)
 * when the package is installed. Falls back gracefully otherwise.
 */
import '@testing-library/jest-dom/vitest';
