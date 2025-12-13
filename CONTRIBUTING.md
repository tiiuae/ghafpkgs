<!--
    SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
    SPDX-License-Identifier: CC-BY-SA-4.0
-->

# Welcome Contributors!

We like commits as they keep the project going. If you have ideas you want to experiment with, make a fork and see how it works. Use pull requests if you are unsure and suggest changes to our maintainers.

- [Welcome Contributors!](#welcome-contributors)
  - [Our Philosophy](#our-philosophy)
  - [Contributing Code](#contributing-code)
    - [Development Process](#development-process)
    - [Commit Message Guidelines](#commit-message-guidelines)
  - [Contributing Documentation](#contributing-documentation)
    - [Working with Documentation Source Files](#working-with-documentation-source-files)
    - [Submitting Changes](#submitting-changes)
    - [Manual of Style](#manual-of-style)
  - [Communication](#communication)


## Our Philosophy

* Update docs with the code.
* Content is King, consistency is Queen.
* Do not assume that readers know everything you currently know.
* Avoid jargon and acronyms, if you can.
* Do not reference future development or features that do not yet exist.


## Contributing Code

### Development Process

Pull requests should be created from personal forks. We follow a fork and rebase workflow.

> The concept of a fork originated with GitHub, it is not a Git concept. If you are new to forks, see [About forks](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/about-forks) and [Contributing Guide when you fork a repository](https://medium.com/@rishabhmittal200/contributing-guide-when-you-fork-a-repository-3b97657b01fb).


### Code Review Process

All code changes must go through our code review process before being merged. This ensures code quality, security, and maintainability.

#### Pull Request Requirements

Before submitting a pull request:

1. **Run Local Checks:**
   ```bash
   # Format code
   nix fmt

   # Check formatting (should show no changes)
   nix fmt -- --fail-on-change

   # Build affected packages
   nix build .#<package-name>

   # Run all checks (recommended before PR)
   nix flake check
   ```

2. **License Headers:**
   - All source files must have SPDX license headers
   - Run `reuse lint` to verify compliance
   - Use Apache-2.0 for code, CC-BY-SA-4.0 for documentation

3. **Code Quality:**
   - No trailing whitespace
   - Follow language-specific style guides
   - Add comments for complex logic
   - Update documentation if needed

#### Submitting a Pull Request

1. **Create a feature branch** from your fork:
   ```bash
   git checkout -b feature/my-feature
   # or
   git checkout -b fix/issue-description
   ```

2. **Make your changes** following our coding standards

3. **Commit with clear messages** (see [Commit Message Guidelines](#commit-message-guidelines))

4. **Push to your fork:**
   ```bash
   git push origin feature/my-feature
   ```

5. **Open a Pull Request** on GitHub:
   - Use a descriptive title
   - Reference any related issues
   - Provide context in the description
   - List what was changed and why

#### PR Template

Use this template for your pull request description:

```markdown
## Description
Brief description of what this PR does

## Related Issues
Fixes #123
Related to #456

## Type of Change
- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Documentation update

## Changes Made
- List key changes
- Be specific about what was modified
- Explain any architectural decisions

## Testing Done
- [ ] Built locally with `nix build`
- [ ] Ran `nix flake check`
- [ ] Tested in Ghaf environment (if applicable)
- [ ] Manual testing performed

## Checklist
- [ ] Code follows project style guidelines
- [ ] SPDX headers added to new files
- [ ] Documentation updated (if needed)
- [ ] Tests pass locally
- [ ] No trailing whitespace (`nix fmt` passed)
```

#### Review Process

1. **Automated Checks:**
   - CI/CD runs format checks, builds, and tests
   - CodeQL security scanning
   - All checks must pass before merge

2. **Peer Review:**
   - At least 1 approval required from maintainers
   - Reviewers check:
     - Code quality and correctness
     - Security implications
     - Test coverage
     - Documentation accuracy
     - Nix best practices

3. **Review Timeline:**
   - Small PRs (< 200 lines): 2-3 days
   - Medium PRs (200-500 lines): 3-5 days
   - Large PRs (> 500 lines): 5-7 days
   - *Consider breaking large PRs into smaller ones*

4. **Addressing Feedback:**
   - Respond to all review comments
   - Push additional commits to address issues
   - Request re-review when ready
   - Mark conversations as resolved when addressed

#### What Reviewers Look For

**Code Quality:**
- Correct implementation
- Error handling
- Edge cases considered
- No code duplication
- Clear variable/function names

**Security:**
- Input validation
- No hardcoded secrets
- Proper authentication/authorization
- Memory safety (for C/C++/Rust)
- Integer overflow checks (for Go/C++)

**Nix Best Practices:**
- No `rec` usage
- Explicit `lib.` prefixes (avoid `with lib`)
- Proper `pname` and `version` attributes
- Correct dependency declarations
- Platform specifications

**Documentation:**
- Code comments for complex logic
- Updated README if needed
- API documentation for public interfaces
- Examples for new features

#### After Approval

Once approved and all checks pass:

1. **Squash commits** if requested
2. **Maintainer merges** (or rebase)
3. **Delete feature branch** after merge
4. **Monitor** for any issues post-merge

#### For Reviewers

When reviewing a PR:

1. **Check the basics:**
   - Does it build? (`nix build`)
   - Do tests pass? (`nix flake check`)
   - Is it formatted? (`nix fmt -- --fail-on-change`)

2. **Read the code:**
   - Understand the changes
   - Check for logic errors
   - Verify error handling
   - Look for security issues

3. **Test locally** if possible:
   ```bash
   # Checkout the PR
   gh pr checkout <number>

   # Test the changes
   nix build .#<affected-package>
   ```

4. **Provide constructive feedback:**
   - Be specific about issues
   - Suggest improvements
   - Explain the reasoning
   - Acknowledge good practices

5. **Approve or request changes:**
   - Approve if ready to merge
   - Request changes if issues found
   - Comment for minor suggestions

#### Getting Help

If you need assistance with the review process:

- Ask questions in the PR comments
- Tag maintainers with `@username`
- Check the [Security Team](./SECURITY.md#security-team) for security-related questions
- Review our [examples of good PRs](#) (coming soon)


### License Headers

Make sure the [license](https://github.com/tiiuae/ghaf#licensing) information is added on top of all your source files as in the example:

    # Copyright [year project started]-[current year], [project founder] and the [project name] contributors
    # SPDX-License-Identifier: Apache-2.0

<!--
# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
-->

Generally, any contributions should pass the tests.

Documentation is the story of your code. Update Ghaf documentation with the code. Good documentation helps to bring new developers in and helps established developers work more effectively.

> Make sure to run spelling checking tools to catch common miss spellings before making a pull request. For example, you can use [aspell](https://www.manuel-strehl.de/check_markdown_spelling_with_aspell) in Linux/UNIX.


### Commit Message Guidelines

We use the Linux kernel compatible commit message format.

The seven rules of a great Git commit message:

1. Separate subject from body with a blank line.
2. Limit the subject line to 50 characters.
3. Capitalize the subject line. If you start subject with a filename, capitalize after colon: “approve.sh: Fix whitespaces”.
4. Do not end the subject line with a period. For example:
5. Use the imperative (commanding) mood in the subject line.
    * ”Fix a bug causing reboots on nuc” rather than “Fixed a bug causing reboots on nuc”.
    * ”Update weston to version 10.5.1” rather than ”New weston version 10.5.1”.
6. Wrap the body at 72 characters.
7. Use the body to explain **what** and **why** vs. how.

Example:
```
Subject line: explain the commit in one line

Body of commit message is a few lines of text, explaining things
in more detail, possibly giving some background about the issue
being fixed, etc etc.

The body of the commit message can be several paragraphs, and
please do proper word-wrap and keep columns shorter than about
72 characters or so. That way "git log" will show things
nicely even when it's indented.

Signed-off-by: Your Name <youremail@yourhost.com>
```

The seven rules of a great Git commit message are originally from Google. Original commit message example is from Linus Torvalds. Both have been modified. Comments and suggestions are welcome.

---

## Contributing Documentation

The Ghaf project is free and open source. We use [Starlight](https://starlight.astro.build) and [Nix](https://nixos.org/manual/nix/stable/introduction.html) for building the documentation and GitHub Pages for hosting. Sources are written in Markdown.

### Working with Documentation Source Files

See the following instructions:

- [Adding New Files](https://github.com/tiiuae/ghaf/blob/main/docs/README-docs.md#adding-new-files) for information on how to manage files/images.
- [Naming](https://github.com/tiiuae/ghaf/blob/main/docs/README-docs.md#naming) for information on file/image naming rules.
- [Managing Content](https://github.com/tiiuae/ghaf/blob/main/docs/README-docs.md#managing-content) for information on how to organize information in chapters, sections, and subsections.


### Submitting Changes

Create a pull request to propose and collaborate on changes to a repository. Please follow the steps below:

1. Fork the project repository.
2. Clone the forked repository to your computer.
3. Create and switch into a new branch with your changes: `git checkout -b doc_my_changes`
4. [Make your changes](#working-with-documentation-source-files).
5. :sunglasses: Check what you wrote with a spellchecker to make sure you did not miss anything.
6. Test your changes before submitting a pull request using the `nix build .#doc` command.
7. Commit your changes: `git commit --signoff`
    - Use "Docs:" in the subject line to indicate the documentation changes. For example: **Docs: rename "Research" to "Research Notes"**.
    - Keep text hard-wrapped at 50 characters.
    - For more inspiration, see [How to Write a Git Commit Message](https://cbea.ms/git-commit/).
8. Push changes to the main branch: `git push origin doc_my_changes`
9. Submit your changes for review using the GitHub UI.
10. After publishing keep your ear to the ground for any feedback and comments in [Pull requests](https://github.com/tiiuae/ghaf/pulls).

When a merge to main occurs it will automatically build and deploy to <https://ghaf.tii.ae>.


### Manual of Style

For information on recommended practices, see [Documentation Style Guide](./docs/style_guide.md).

---

## Communication

GitHub issues are the primary way for communicating about specific proposed changes to this project.

If you want to join the project team, just let us know.
