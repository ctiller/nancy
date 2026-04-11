# 0049. Human-in-the-Loop Ask Metrics

## Title
Human-in-the-Loop Ask Metrics DAG Topology

## Context
Agents encounter tasks that are fundamentally impossible to fulfill without accessing hidden human operator context or explicitly seeking human clarity. Without an interruption vector, LLM loops waste tokens retrying or aggressively hallucinating solutions. A strictly observable schema was needed that securely connects humans back into the execution loop asynchronously.

## Decision
We introduced three new `EventPayload` structures to our task registry: `Ask`, `CancelAsk`, and `HumanResponse`. These nodes act as bidirectional requests within the immutable DAG namespace. Instead of freezing or failing, agents invoke an `investigate` tool wrapper that dynamically issues an `Ask` node referencing their exact state, agent path, and a specific textual question for the human operator. 
Agents automatically enter a long-polling boundary, pausing inference safely until a Human Operator acknowledges the `Ask` node by writing back a `HumanResponse` explicitly addressing that particular `ask_ref`, optionally canceling it natively.

## Consequences
1. Agents will poll indefinitely or wait securely, lowering execution bounds.
2. Web interfaces (`AppView`) inherently assume the obligation to detect `Ask` payloads dynamically and pulse/alert human operators visually.
3. Tests and orchestration loops must support or explicitly skip human delays via mocking mechanisms `NANCY_HUMAN_DID` constraints.
