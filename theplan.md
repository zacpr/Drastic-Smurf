***The Plan***
The goal is the creation of an extensible tool for interacting with and monitoring elasticsearch.
The app msut support multiple clusters with separate credentials and potentially differnt auth methods
so the overarching app will maintain the cluster information, and authentication and privide a tabbed interface for interacting with the vrious modules.

**Expected Modules**
- Snapshot monitoring; this should replicate (or port over etc) the functionality available in /home/zac/app_dev/es-snap-mon/
- Cluster task monitoring; this will mainly provide a way to monitor reindex operations,but shuould provide a way to view oth3er task types , with filtering per type etc
- Cluster status monitoring; this should provide status and health information in a dashboaard formqt. it shoudl be able to provide an overview of all clustrers , a select subset of clusters,  or a more detailed single cluster view
- Elastic/Kibana console; this should provide fucntionaluty similar to the devtools console in kibana , allowing the user to interact withthe api without re-entering credentials or clsuter details


Technical; performqance and responisveness is key, if feasible the project shoudl be written in rust
