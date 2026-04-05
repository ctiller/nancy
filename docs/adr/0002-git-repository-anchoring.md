# ADR 0002: Git Repository Anchoring

## Status

Accepted

## Context

`nancy` requires persistent state and context to operate effectively. Instead of tracking configurations globally or arbitrarily on the user's system, we need a reliable way to scope a unique `nancy` environment to the specific development context the user is working in. 

## Decision

We will anchor `nancy`'s state generation directly into the local `git` repository in which it is initialized. 
- The `git2` crate is used to discover the root working directory computationally (`git2::Repository::discover`).
- Information is stored within a `.nancy/` configuration folder located in the `.git` parent wrapper (the workdir root).
- Upon initialization, `.nancy` is automatically added to the `.gitignore` of the repository to prevent the user from accidentally committing sensitive metadata or private keys.

## Consequences

- **Positive:** Context is automatically scoped to a specific project.
- **Positive:** The tool immediately halts with a meaningful error if run outside of a valid checkout or within a bare repository.
- **Negative:** Users working on non-git projects will not be able to use `nancy` without first using `git init`.
