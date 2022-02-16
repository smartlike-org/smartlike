[![Blog](https://img.shields.io/badge/blog-blog%2esmartlike%2eorg-blue.svg?style=flat-square)](https://smartlike.org/channel/blog.smartlike.org)
[![Forum](https://img.shields.io/badge/forum-github%20discussions-blue.svg?style=flat-square)](https://github.com/smartlike-org/smartlike/discussions)
[![Project](https://img.shields.io/badge/explore-smartlike%2eorg-blue.svg?style=flat-square)](https://smartlike.org/)
[![License: AGPL 3](https://img.shields.io/badge/license-AGPL%203-blue.svg)](https://github.com/smartlike-org/smartlike/LICENSE)

A Fediverse to Smartlike relay prototype.

Micro-donations are accumulated per content item and shared between creators and hosting instances ([example](https://smartlike.org/channel/buddhist.tv)). To get paid out, users connect their Fediverse to Smartlike accounts by mentioning their Smartlike account in the Fediverse profile description ([more details](https://smartlike.org/docs/how-to-set-up-content-monetization)). Instance administrators add the signature as a DNS entry and choose what share they would like to receive.

When users connect their accounts, they can opt to have their Fediverse keys trusted on Smartlike in order to make one-click micro-donations when they click thumb-up/upvote/reblog/etc in Fediverse. An instance of this relay transparently forwards signed transactions from connected users to Smartlike where they are turned into micro-donations.

What we seek to achieve:
- **More people to join Fediverse**. Popular content creators will get an additional incentive to publish in Fediverse, attracting their audience to join.
- **Fediverse sustainability and scalability**. Instance administrators will have an option to take a cut to cover their hosting costs without asking for donations via a centralized service.
- **Improve user experience**. Anonymous donation accumulators are stored on a public ledger and can be used by content apps to enhance feeds, charts, search and recommendations without robbing users of their freedom and privacy or building walls between platforms. Tags produced by [decentralized crowdsourced moderation](https://smartlike.org/docs/censorship-and-moderation) are also shared to let users choose what filters to set and what moderators to trust.

Currently supported Fediverse projects:

-   PeerTube
-   Mastodon

## Contribute

Smartlike is an open source project. We welcome all sorts of participation. If you can connect a Fediverse project you care about, the community will appreciate it. Let's discuss on our [forum](https://discuss.smartlike.org).

## License

[![License: AGPL 3](https://img.shields.io/badge/License-AGPL%203-blue.svg)](https://github.com/smartlike-org/smartlike/LICENSE)

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.
This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
