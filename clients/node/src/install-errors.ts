import { EokaError, type ErrorDetails } from './errors.js';

export class InstallationError extends EokaError {
  constructor(code: string, message: string, details: ErrorDetails = {}) {
    super(code, message, details);
  }
}
