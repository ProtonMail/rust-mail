# RFC

We will use [Request for Comments](https://www.ietf.org/standards/rfcs/) (RFC) simplified for our needs to implement significant changes.

Some older RFC are present in the archived [documentation repo](https://gitlab.protontech.ch/rust/documentation).
New RFC should be submitted to this repository.

## Requirements

* RFCs is held in a git repository, as Markdown.
    * If it requires collaboration with people without Git access (product, design, etc.), Confluence or other tool can be used, but end result is migrated to this repository.
* [Template](./YYYY-MM-DD-TEMPLATE.md) is used to create new RFC (manually create new file in this folder and copy the content of the template).
    * All sections are kept and N/A is written when not applicable. A reason for this is that anyone who reviews the RFC also agrees section is not applicable (e.g., that author didn't forget to consider something).
    * Descriptions of sections are kept intact. Same reason as above: even reviewer can simply see what given section is about and can help find out if author didn't forget to include some answers.
* Every RFC has MR that can be used for discussion.
    * It is OK to merge sooner and open new MR for easier management.
* All changes are only in the RFC file (folder can be used for bigger changes).
    * Changes to the documentation needed to reflect the adoption of the RFC will be made in a companion merge request.
    * After RFC is accepted, changes to the whole documentation can happen.
* Every RFC must have at least two approvers.

## Flow

* Author creates new file in [here](./) with a copy of the [template](./YYYY-MM-DD-TEMPLATE.md)
* Author fills all the relevant sections
* Once done, author creates new branch and MR
    * Branch is called **rfc/{short-name}**
    * MR title includes **rfc: {title}**
* Author selects two reviewers, the approvers, for deep review
    * The approvers should be involved with the topic already
    * The approvers should be diverse and bring good breadth to the review
        * For example, a client developer writing an RFC would ask a backend member of the feature team and someone from the foundation team
* Reviewers focus on the operational guidelines:
    * implementation feasability
    * operational metrics
    * testing requirements
* Author gathers feedback and resolves issues within one week
* Author shares MR in `#et-rust` channel
* Open for feedback from anyone who is interested
    * Limited to one week by default
    * Only one person from the team should read by default
* Once approvers accept, author updates documentation as necessary
  * Rejected RFC move to the [rejected folder](./rejected)
  * Rejected RFC's should include an explantation as to why they were rejected

## RFCs @ Proton

Proton has process to approve designs of significant changes by a comittee. Please follow this page for guidance:

https://confluence.protontech.ch/display/QR/Quality+and+reliability+home
