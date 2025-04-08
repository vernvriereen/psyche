export function createRateLimitedPromise<T extends (...args: any[]) => Promise<any>>(
  promiseFn: T,
  maxCalls: number,
  timeWindowMs: number
): T {
  
  interface QueuedRequest {
    args: Parameters<typeof promiseFn>;
    resolve: (value:Awaited<ReturnType<T>>) => void;
    reject: (reason?: any) => void;
  }

  const queue: QueuedRequest[] = [];
  const completionTimes: number[] = [];
  let inFlightCount = 0;

  const processQueue = (): void => {
    // drop timestamps older than our rate limit window
    const now = Date.now();
    while (completionTimes.length && completionTimes[0] <= now - timeWindowMs) {
      completionTimes.shift();
    }

    // fire every request we have space for in our queue
    while (queue.length && (completionTimes.length + inFlightCount) < maxCalls) {
      const request = queue.shift()!;
      inFlightCount++;
      
      Promise.resolve(promiseFn(...request.args))
        .then((result) => request.resolve(result))
        .catch((error) => request.reject(error))
        .finally(() => {
          inFlightCount--;
          completionTimes.push(Date.now());
 
          // if we're the first one to complete,
          // schedule processing the queue again after the time window
          if (queue.length && completionTimes.length === 1) {
            setTimeout(processQueue, timeWindowMs);
          }
        });
    }

    // check the queue on the next expiration
    if (queue.length && completionTimes.length) {
      const nextProcessTime = completionTimes[0] + timeWindowMs - now;
      setTimeout(processQueue, nextProcessTime);
    }
  };

  return function rateLimitedPromise(
    ...args: Parameters<typeof promiseFn>
  ) {
    return new Promise((resolve, reject) => {
      queue.push({
        args,
        resolve,
        reject
      });
      
      processQueue();
    });
  } as any;
}
export function makeRateLimitedFetch(): typeof fetch {
	const MAX_RETRIES = 10
	const RETRY_INIT_TIME_MS = 1000
	const RETRY_MULT = 2.5
  const REQS_PER_SECOND = 40;
	const rateLimitedFetch = createRateLimitedPromise(fetch, REQS_PER_SECOND, 1000)

	return async function (
		...args: Parameters<typeof fetch>
	): ReturnType<typeof fetch> {
		let attempts = 0
		while (true) {
			attempts += 1
			try {
				const result = await rateLimitedFetch(...args)
				if (result.ok) {
					return result
				} else {
					// throws to catch
					await result.text().then((body) => {
						throw new Error(
							`fetch of ${args[0]} failed. status: ${result.status} - ${body}`
						)
					})
				}
			} catch (err) {
				if (attempts >= MAX_RETRIES) {
					throw err
				}
				// retry by looping!
				const retryDelay = RETRY_INIT_TIME_MS * RETRY_MULT ** attempts
				console.warn(`retrying after ${retryDelay}...`)
			}
		}
	}
}
