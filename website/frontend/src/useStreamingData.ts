import { useLoaderData } from '@tanstack/react-router'
import { useState, useEffect, useRef } from 'react'

export function useStreamingLoaderData<T extends object>(
	...loaderData: Parameters<typeof useLoaderData>
) {
	const stream = useLoaderData(...loaderData)
	const [data, setData] = useState<(T & { disconnected: boolean }) | null>(null)

	const lastData = useRef<T | null>(null)

	const previousReaderDone = useRef<Promise<void> | null>(null)
	useEffect(() => {
		let signalDone!: () => void
		const doneLoop = new Promise<void>((r) => {
			signalDone = r
		})

		const prevReaderDonePromise = previousReaderDone.current

		async function readStream() {
			await prevReaderDonePromise
			const reader = stream.getReader()
			try {
				if (lastData.current) {
					setData({ ...lastData.current, disconnected: false })
				}
				while (true) {
					const res = await Promise.race([reader.read(), doneLoop])
					if (!res) {
						break
					}
					const { value, done } = res
					if (done) break
					console.log(`got new streaming data for ${loaderData[0].from}`)
					setData({ ...value, disconnected: false })
					lastData.current = value
				}
			} catch (err) {
				console.error('Error reading stream:', err)
			} finally {
				reader.releaseLock()
				if (lastData.current) {
					setData({ ...lastData.current, disconnected: true })
				}
			}
		}

		previousReaderDone.current = readStream()

		return () => {
			signalDone()
		}
	}, [stream])

	return data
}
