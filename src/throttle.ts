export interface Throttled {
  call: () => void;
  cancel: () => void;
}

/** Runs `fn` at once, then at most once per `ms` while calls keep coming, ending with the last one. */
export function throttle(fn: () => void, ms: number): Throttled {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending = false;

  const call = () => {
    if (timer !== undefined) {
      pending = true;
      return;
    }
    fn();
    timer = setTimeout(() => {
      timer = undefined;
      if (pending) {
        pending = false;
        call();
      }
    }, ms);
  };

  return {
    call,
    cancel: () => {
      clearTimeout(timer);
      timer = undefined;
      pending = false;
    },
  };
}
