interface PyscheNode {
  location: { lat: number; lon: number };
  connections: string[];
  index: number;
  id: string;
}

export interface PsycheStats {
  nodes: PyscheNode[];
  coordinator: Coordinator;
}

export interface Coordinator {
  runId: string;

  startTime: Date;

  batchesPerRound: number;
  roundHeight: number;
  totalBatches: number;
  tokensPerBatch: number;

  stats: StepStats[];

  epoch: number;
}

export interface StepStats {
  step: number;
  evals: Record<string, number | undefined>;
  certainty: number;
  loss: number;
  tokensPerSecond: number;
}
