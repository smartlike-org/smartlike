# Smartlike

[![Blog](https://img.shields.io/badge/blog-blog%2esmartlike%2eorg-blue.svg?style=flat-square)](https://smartlike.org/channel/blog.smartlike.org)
[![Forum](https://img.shields.io/badge/forum-github%20discussions-blue.svg?style=flat-square)](https://github.com/smartlike-org/smartlike/discussions)
[![Project](https://img.shields.io/badge/explore-smartlike%2eorg-blue.svg?style=flat-square)](https://smartlike.org/)
[![License: AGPL 3](https://img.shields.io/badge/license-AGPL%203-blue.svg)](https://github.com/smartlike-org/smartlike/LICENSE)

Smartlike builds a free non-profit decentralized donation processor with a focus on freedom, privacy and efficiency:

-   users choose moderation policies, creators and publishers get more visibility and independence from middlemen
-   no registration, no private data collection
-   zero commission, high bandwidth

Our mission is to help decentralize the Internet, develop common good technologies for people and businesses to thrive in direct cooperation.

The service employs a hybrid from trusted relationships between creators and their audience for private and secure end-to-end payments and trust-less horizontally scalable public ledger technology for transaction processing without cryptocurrency.

## Roadmap

### 2016 - Blockchain

-   [an alternative](https://medium.com/@vadim.frolov/thank-u-value-and-money-redefined-on-blockchain-to-fix-ad-blocking-79de7a87231c#.qu2w33zeh) to ad-sponsored content distribution
-   blockchain but no middlemen miners or volatility for speculation

### 2019 - Distributed Hash Table (DHT)

-   eventually got rid of blocks and moved to a validating DHT for storage
-   experimented with one of the most promising DHT implementations - [Holochain](https://github.com/holochain)

### 2021 - Free nonprofit donation processor that scales

-   all advertized features with decentralized privacy, financial security high bandwidth and zero commission
-   **but**: since there was still no fitting production grade DHT around, a centralized but scalable and proven Kafka-Cassandra cluster was used to serve as a temporary solution that transparently stores all signed transactions on BitTorrent

### 2022-23 - Exit to community

-   switch to a DHT network
-   hand over the control to a meritocratic community governance (users who benefit most should drive the system)
-   a zero exit is targeted to cover development costs by donations

## Contribute

Smartlike is an open source project. We welcome all sorts of participation. Let's discuss on our [forum](https://github.com/smartlike-org/smartlike/discussions)

Please consider donating via [smartlike](https://smartlike.org/donate) to get the [ball rolling](https://blog.smartlike.org/how-it-works):

-   the donation is immediately used for development
-   the same amount is credited to your Smartlike account to be used for micro-donations
-   the amount is also charged to our Smartlike account which produces a negative balance
-   the negative balance will be removed from future micro-donations to Smartlike development

## License

[![License: AGPL 3](https://img.shields.io/badge/License-AGPL%203-blue.svg)](https://github.com/smartlike-org/smartlike/LICENSE)

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.
This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
