type Method = (this: never, ...args: never[]) => unknown;
type CallSite = { stack?: string };

const wrapperFrame = 'eokaTracedCall';
let depth = 0;

function capture(boundary: Method): CallSite {
  const site: CallSite = {};
  Error.captureStackTrace(site, boundary);
  return site;
}

function extendStack(error: unknown, site: CallSite): unknown {
  if (!(error instanceof Error) || typeof error.stack !== 'string' || typeof site.stack !== 'string') return error;
  const own = error.stack.split('\n').filter(line => !line.includes(wrapperFrame));
  const caller = site.stack.split('\n').slice(1).filter(line => !line.includes(wrapperFrame) && !own.includes(line));
  const stack = [...own, ...caller].join('\n');
  if (stack !== error.stack) error.stack = stack;
  return error;
}

export function withCallSite<T>(boundary: Method, run: () => Promise<T>): Promise<T> {
  const site = capture(boundary);
  return run().catch((error: unknown) => { throw extendStack(error, site); });
}

export function traced<F extends Method>(method: F): F {
  const wrapper = function eokaTracedCall(this: never, ...args: never[]): unknown {
    if (depth > 0) return method.apply(this, args);
    const site = capture(wrapper);
    depth++;
    let result: unknown;
    try {
      result = method.apply(this, args);
    } finally {
      depth--;
    }
    return result instanceof Promise ? result.catch((error: unknown) => { throw extendStack(error, site); }) : result;
  };
  return wrapper as F;
}

export function traceCallSites(owner: object, skip: readonly string[] = []): void {
  for (const name of Object.getOwnPropertyNames(owner)) {
    const descriptor = Object.getOwnPropertyDescriptor(owner, name);
    if (name === 'constructor' || skip.includes(name) || typeof descriptor?.value !== 'function') continue;
    Object.defineProperty(owner, name, { ...descriptor, value: traced(descriptor.value as Method) });
  }
}
