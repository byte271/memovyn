export class MemovynError extends Error {
  readonly code: string;

  constructor(message: string, code = "memovyn_error") {
    super(message);
    this.code = code;
    this.name = "MemovynError";
  }
}

export class InvalidRequestError extends MemovynError {
  constructor(message: string) {
    super(message, "invalid_request");
  }
}

export class NotFoundError extends MemovynError {
  constructor(message: string) {
    super(message, "not_found");
  }
}
