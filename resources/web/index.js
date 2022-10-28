const apiHost = ''
// const apiHost = 'http://localhost:8017'

const Utils = {
    plural(n, singular, plural) {
        const noun = n === 1 ? singular : plural
        return `${n} ${noun}`
    },
    pluralS(n, singular) {
        return Utils.plural(n, singular, `${singular}s`)
    },
    formatDuration(seconds) {
        if (seconds < 1) {
            return `${Math.round(seconds * 1e3)}ms`
        } else if (seconds < 60) {
            return `${seconds.toFixed(1)}s`
        } else {
            return `${(seconds / 60).toFixed(1)}min`
        }
    },
}

const appComponent = Vue.createApp({
    template: '#app',
    data() {
        return {
            loaded: false,
            loading: false,
            lastUpdate: new Date,
            name: null,
            columns: [],
            detailedIssueKey: null,
            detailedIssue: null,
        }
    },
    mounted() {
        const modalEl = document.getElementById('issue-detail-modal')
        this.issueDetailModal = new bootstrap.Modal(modalEl, {})
        modalEl.addEventListener('hide.bs.modal', () => {
            this.detailedIssueKey = null
            this.detailedIssue = null
        })
    },
    methods: {
        ...Utils,
        async update() {
            if (this.loading) {
                return
            }

            this.loading = true
            try {
                const response = await (await fetch(`${apiHost}/api/board`)).json()
                this.name = response.name
                this.columns = response.columns

                if (this.detailedIssueKey !== null) {
                    await this.updateDetailedIssue()
                }

                this.loaded = true
                this.lastUpdate = new Date
            } finally {
                this.loading = false
            }
        },
        openIssue(key) {
            this.issueDetailModal.show()
            this.detailedIssueKey = key
            this.detailedIssue = null
            this.updateDetailedIssue().catch(console.error)
        },
        async updateDetailedIssue() {
            const response = await (await fetch(`${apiHost}/api/issue/${this.detailedIssueKey}`)).json()

            if (response.key === this.detailedIssueKey) {
                this.detailedIssue = response
            }
        }
    }
})

appComponent.component('relative-date', {
    props: ['date'],
    template: '#relative-date',
    data() {
        return {
            interval: null,
            text: '',
        }
    },
    created() {
        this.updateLoop()
    },
    destroyed() {
        clearInterval(this.interval)
    },
    methods: {
        updateLoop() {
            const date = this.date instanceof Date ? this.date : new Date(this.date)
            const deltaSeconds = (Date.now() - date.getTime()) / 1e3
            let tickLength
            if (deltaSeconds < 60) {
                this.text = `${Utils.pluralS(deltaSeconds.toFixed(0), 'second')} ago`
                tickLength = 1e3
            } else if (deltaSeconds < 3600) {
                this.text = `${Utils.pluralS((deltaSeconds / 60).toFixed(1), 'minute')} ago`
                tickLength = 6e3
            } else {
                this.text = `${Utils.pluralS((deltaSeconds / 3600).toFixed(1), 'hour')} ago`
                tickLength = 360e3
            }
            this.interval = setTimeout(() => this.updateLoop(), tickLength)
        }
    }
})

appComponent.component('board-column', {
    props: ['name', 'issues'],
    template: '#board-column',
})

appComponent.component('board-issue', {
    props: ['issueKey', 'summary', 'status', 'avatars', 'epic', 'branches', 'mergeRequests'],
    template: '#board-issue',
    methods: {
        ...Utils,
    }
})

const app = appComponent.mount('#main')

let nextUpdateTimer = null

function updateAndLoop() {
    if (app.loading) {
        return
    }

    clearTimeout(nextUpdateTimer)
    app.update().catch(error => console.error(error)).finally(() => {
        const sleepTime = document.visibilityState === 'visible' ? 10e3 : 90e3
        nextUpdateTimer = setTimeout(updateAndLoop, sleepTime)
    })
}

document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') {
        updateAndLoop()
    }
})

updateAndLoop()
