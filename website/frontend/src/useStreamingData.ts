import { useLoaderData } from '@tanstack/react-router'
import { useState, useEffect, useRef } from 'react'

export function useStreamingRunData() {
	const { initialData, stream } = useLoaderData({ from: '/runs/$run' })
	const [data, setData] = useState(initialData)

	const previousReaderDone = useRef<Promise<void> | null>(null)
	useEffect(() => {
		setData(initialData)

		let signalDone!: () => void
		const doneLoop = new Promise<void>((r) => {
			signalDone = r
		})

		const prevReaderDonePromise = previousReaderDone.current

		async function readStream() {
			await prevReaderDonePromise
			const reader = stream.getReader()
			try {
				while (true) {
					const res = await Promise.race([reader.read(), doneLoop])
					if (!res) {
						break
					}
					const { value, done } = res
					if (done) break
					setData(value)
				}
			} catch (err) {
				console.error('Error reading stream:', err)
			} finally {
				reader.releaseLock()
			}
		}

		previousReaderDone.current = readStream()

		return () => {
			signalDone()
		}
	}, [initialData, stream])

	return data
}
