# VARIABLES
variable "REGISTRY" {
  default = "ghcr.io"
}

variable "REPO" {
  default = "mysteryforge/eth-kit"
}

variable "GIT_SHA" {
  default = "undefined"
}

# GROUPS
group "default" {
  targets = ["all"]
}

group "all" {
  targets = [
    "execution-probe",
    "beacon-probe",
  ]
}

# DISPERSER TARGETS
target "execution-probe" {
  context    = "."
  dockerfile = "./Dockerfile"
  target     = "execution-probe"
  tags       = ["${REGISTRY}/${REPO}/execution-probe:${GIT_SHA}"]
}

target "beacon-probe" {
  context    = "."
  dockerfile = "./Dockerfile"
  target     = "beacon-probe"
  tags       = ["${REGISTRY}/${REPO}/beacon-probe:${GIT_SHA}"]
}
