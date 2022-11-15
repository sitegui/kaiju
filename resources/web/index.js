'use strict'

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
        }
    },
    methods: {
        ...Utils,
        async update() {
            if (this.loading) {
                return
            }

            this.loading = true
            try {
                const response = await (await fetch('/api/board')).json()
                this.name = response.name
                this.columns = response.columns

                await this.$refs.issueDetails.update()

                this.loaded = true
                this.lastUpdate = new Date
            } finally {
                this.loading = false
            }
        },
        openIssue(key) {
            this.$refs.issueDetails.open(key)
        },
        startCreation() {
            this.$refs.issueEditor.create()
        },
        startEdit(key) {
            this.$refs.issueEditor.edit(key)
        }
    }
})

appComponent.component('issue-details', {
    props: [],
    template: '#issue-details',
    data() {
        return {
            // General state
            issueKey: null,
            loaded: false,
            modal: null,
            // Details
            avatars: null,
            branches: null,
            description: null,
            epic: null,
            jiraLink: null,
            mergeRequests: null,
            status: null,
            summary: null,
        }
    },
    mounted() {
        const modalEl = this.$refs.modal
        this.modal = new bootstrap.Modal(modalEl, {})
        modalEl.addEventListener('hide.bs.modal', () => {
            this.issueKey = null
        })
    },
    methods: {
        open(key) {
            this.modal.show()
            this.issueKey = key
            this.loaded = false
            this.update().catch(console.error)
        },
        startEdit() {
            this.$emit('editIssue', this.issueKey)
            this.modal.hide()
        },
        async update() {
            if (this.issueKey === null) {
                return
            }

            const response = await (await fetch(`/api/issue/${this.issueKey}`)).json()

            if (response.key === this.issueKey) {
                this.loaded = true
                this.avatars = response.avatars
                this.branches = response.branches
                this.description = response.description
                this.epic = response.epic
                this.jiraLink = response.jira_link
                this.mergeRequests = response.merge_requests
                this.status = response.status
                this.summary = response.summary
            }
        },
    }
})

appComponent.component('issue-editor', {
    props: [],
    template: '#issue-editor',
    data() {
        return {
            issueKey: null,
            modal: null,
            editor: null,
            saving: false,
        }
    },
    mounted() {
        this.modal = new bootstrap.Modal(this.$refs.modal, {})

        this.editor = ace.edit(this.$refs.editor)
        this.editor.setTheme('ace/theme/textmate')
        this.editor.session.setMode('ace/mode/markdown')
        this.editor.session.setUseWrapMode(true)
        this.editor.commands.addCommand({
            name: 'save-issue',
            bindKey: {win: 'Ctrl-Enter', mac: 'Command-Enter'},
            exec: () => this.save(),
            readOnly: true,
        })
    },
    methods: {
        create() {
            this.modal.show()
            this.editor.setValue('Loading...', -1)
            this.editor.setReadOnly(true)
            setInterval(() => this.editor.renderer.updateFull(), 0)
            this.issueKey = null
            this.saving = false

            fetch('/api/new-issue-code').then(response => response.text()).then(issueCode => {
                if (this.issueKey === null) {
                    this.editor.setValue(issueCode, -1)
                    this.editor.setReadOnly(false)
                }
            }).catch(console.error)
        },
        edit(key) {
            this.modal.show()
            this.editor.setValue('Loading...', -1)
            this.editor.setReadOnly(true)
            setInterval(() => this.editor.renderer.updateFull(), 0)
            this.issueKey = key
            this.saving = false

            fetch(`/api/edit-issue-code/${key}`).then(response => response.text()).then(issueCode => {
                if (this.issueKey === key) {
                    this.editor.setValue(issueCode, -1)
                    this.editor.setReadOnly(false)
                }
            }).catch(console.error)
        },
        save() {
            this._save().catch(console.error)
        },
        async _save() {
            this.editor.setReadOnly(true)
            this.saving = true

            const url = this.issueKey === null ? '/api/issue' : `/api/issue/${this.issueKey}`
            const code = this.editor.getValue()

            try {
                const response = await fetch(url, {method: 'POST', body: code})
                if (!response.ok) {
                    const body = await response.text()
                    throw new Error(`Call failed with status ${response.status}:\n${body}`)
                }
                this.modal.hide()
                this.$emit('issue-created')
            } catch (error) {
                const errorLines = String(error).split('\n').map(line => `-- ${line}`)
                this.editor.setValue(errorLines.join('\n') + '\n\n' + code)
            } finally {
                this.saving = false
                this.editor.setReadOnly(false)
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
    props: ['name', 'issues', 'isLast'],
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
